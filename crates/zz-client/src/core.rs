use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::Arc,
};

use zz_protocol::{
    AgentPaneWire, BrowserCommand, ChooseBufferSearchState, ChooseBufferState,
    ChooseTreeSearchState, ChooseTreeState, ChooserPresentation, ClientExitAction,
    ClientMessageKind, ClientView, ClipboardProducer, CommandPromptState, CommandResponse,
    ConfirmState, DisplayPanesState, Event, EventPayload, KeyBindingSnapshot, KeyTableSnapshot,
    MenuState, MouseBindings, MuxOptions, MuxSnapshot, PaneId, ProtocolMessage, ServerHello,
    SessionId, SimulatorDescriptor, StatusLine, TerminalUiCommand, Welcome, key_tables_hash,
};
use zz_terminal::{
    AppearanceProvenance, ClipboardTarget, PackedCell, TerminalAppearance, TerminalDictionary,
    TerminalPatchFields, TerminalViewport, TerminalViewportPatch,
};

use crate::scrollback::{
    HistoryRing, MAX_HISTORY_ROWS, RetainedTerminalViewport, apply_history_chunk,
    apply_retained_patch, new_retained_viewport, replace_retained_viewport,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewportDamage {
    All,
    Rows(Vec<u16>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outbound {
    RequestFull(PaneId),
    TreeSync,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAttentionStatus {
    Idle,
    Working,
    NeedsInput,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAttentionEdge {
    Request,
    Done,
    Failed,
}

#[must_use]
pub fn agent_attention_status(state: &AgentPaneWire) -> AgentAttentionStatus {
    if state.pending_permission.is_some() {
        return AgentAttentionStatus::NeedsInput;
    }
    match &state.phase {
        zz_protocol::AgentConnectionPhase::Running
        | zz_protocol::AgentConnectionPhase::AwaitingPermission => AgentAttentionStatus::Working,
        zz_protocol::AgentConnectionPhase::Failed { .. } => AgentAttentionStatus::Failed,
        zz_protocol::AgentConnectionPhase::Starting | zz_protocol::AgentConnectionPhase::Ready => {
            AgentAttentionStatus::Idle
        }
    }
}

fn agent_attention_edge(
    previous: &AgentPaneWire,
    current: &AgentPaneWire,
) -> Option<AgentAttentionEdge> {
    let previous = agent_attention_status(previous);
    let current = agent_attention_status(current);
    match (previous, current) {
        (previous, AgentAttentionStatus::NeedsInput)
            if previous != AgentAttentionStatus::NeedsInput =>
        {
            Some(AgentAttentionEdge::Request)
        }
        (AgentAttentionStatus::Working, AgentAttentionStatus::Idle) => {
            Some(AgentAttentionEdge::Done)
        }
        (previous, AgentAttentionStatus::Failed) if previous != AgentAttentionStatus::Failed => {
            Some(AgentAttentionEdge::Failed)
        }
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CoreEvent {
    HelloReceived,
    Attached {
        session: SessionId,
    },
    SnapshotChanged,
    ViewportChanged {
        pane: PaneId,
        damage: ViewportDamage,
    },
    AppearanceChanged,
    MuxOptionsChanged,
    KeyTablesChanged,
    StatusChanged,
    PrefixArmed {
        armed: bool,
    },
    PrefixCancelled {
        request_id: u64,
    },
    KeyTableChanged,
    CommandPromptChanged,
    CommandOutputChanged,
    ChooseTreeChanged,
    ChooseBufferChanged,
    DisplayPanesChanged,
    MenuChanged,
    ConfirmChanged,
    PaneRemoved {
        pane: PaneId,
    },
    Bell {
        pane: PaneId,
    },
    FocusSidebar,
    OpenPathPicker {
        pane: PaneId,
        start_dir: Option<String>,
    },
    Detached {
        session: SessionId,
        by: Option<String>,
        action: ClientExitAction,
    },
    ServerStopping,
    CommandResponse(CommandResponse),
    ClientMessage {
        pane: Option<PaneId>,
        kind: ClientMessageKind,
        text: String,
        duration_ms: Option<u32>,
        message_id: Option<u64>,
    },
    ClientMessageCleared {
        message_id: u64,
    },
    Clipboard {
        pane: PaneId,
        request_id: u64,
        target: ClipboardTarget,
        text: String,
        producer: ClipboardProducer,
    },
    OpenUri {
        pane: PaneId,
        uri: String,
    },
    BrowserCommand {
        pane: PaneId,
        command: BrowserCommand,
    },
    SimulatorChanged {
        pane: PaneId,
    },
    SimulatorCommand {
        pane: PaneId,
        command: zz_protocol::SimulatorCommand,
    },
    TerminalUiCommand {
        pane: PaneId,
        command: TerminalUiCommand,
    },
    HistoryChunk {
        pane: PaneId,
        start: u32,
        total: u32,
        offset: u32,
        columns: u16,
        rows: Vec<Vec<PackedCell>>,
        dictionary: TerminalDictionary,
    },
    KittyImageBegin {
        pane: PaneId,
        image_id: u32,
        generation: u64,
        width: u32,
        height: u32,
        total_bytes: u32,
    },
    KittyImageChunk {
        pane: PaneId,
        image_id: u32,
        generation: u64,
        bytes: Vec<u8>,
    },
    KittyImagesRemoved {
        pane: PaneId,
        image_ids: Vec<u32>,
    },
    AgentUpdates {
        pane: PaneId,
        first_seq: u64,
        items: Vec<Vec<u8>>,
    },
    AgentStateChanged {
        pane: PaneId,
        attention: Option<AgentAttentionEdge>,
    },
    AgentLagged {
        pane: PaneId,
        next_seq: u64,
    },
    AgentReplayed {
        pane: PaneId,
        through_seq: u64,
    },
    AgentSessions {
        pane: PaneId,
        request_id: u64,
        result: String,
    },
    ProvidersChanged,
    AgentAccountsChanged,
    AgentLoginChanged {
        provider: zz_protocol::AgentProvider,
    },
    Message(Box<ProtocolMessage>),
}

type PaneMap<V> = HashMap<PaneId, V, foldhash::fast::FixedState>;

#[derive(Debug)]
struct PrefixKeys([Option<String>; 2]);

impl PrefixKeys {
    fn from_options(options: &MuxOptions) -> Self {
        Self(
            [
                zz_protocol::MuxOptionKey::Prefix,
                zz_protocol::MuxOptionKey::Prefix2,
            ]
            .map(|key| {
                options
                    .get(key)
                    .filter(|option| !option.value.eq_ignore_ascii_case("none"))
                    .map(|option| zz_protocol::canonical_key(&option.value))
            }),
        )
    }

    fn matches(&self, key: &str) -> bool {
        self.0.iter().flatten().any(|prefix| prefix == key)
    }
}

impl Default for PrefixKeys {
    fn default() -> Self {
        Self::from_options(&MuxOptions::default())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreDetachReason {
    Requested,
    Evicted,
    SessionDestroyed,
    ServerStopping,
}

#[derive(Debug, Default)]
pub struct ClientCore {
    hello_received: bool,
    server_id: u64,
    server_revision: u32,
    server_clock: u64,
    host_platform: zz_protocol::HostPlatform,
    capabilities: Vec<String>,
    appearance: Option<Box<TerminalAppearance>>,
    appearance_provenance: AppearanceProvenance,
    mux_options: MuxOptions,
    terminal_negotiation: Option<(Vec<String>, Vec<String>, Vec<String>)>,
    prefix_keys: PrefixKeys,
    key_tables: Vec<KeyTableSnapshot>,
    key_tables_hash: u64,
    mouse_bindings: MouseBindings,
    status: StatusLine,
    snapshot: Arc<MuxSnapshot>,
    tree: MuxSnapshot,
    client_view: ClientView,
    tree_sync_pending: bool,
    batch_reducing: bool,
    event_group_start: usize,
    tree_dirty: bool,
    attached_session: Option<SessionId>,
    attached_read_only: bool,
    attached_client_flags: String,
    last_detach_reason: Option<CoreDetachReason>,
    viewports: PaneMap<RetainedTerminalViewport>,
    spare_cells: PaneMap<SpareCells>,
    copy_generations: PaneMap<u64>,
    next_row_revision: u64,
    history_limit: usize,
    agent_states: HashMap<PaneId, AgentPaneWire>,
    simulators: HashMap<PaneId, SimulatorDescriptor>,
    agent_parents: BTreeMap<PaneId, PaneId>,
    providers: Option<zz_protocol::ProvidersReport>,
    agent_accounts: Option<Vec<zz_protocol::AgentAccount>>,
    agent_quotas: Vec<zz_protocol::AgentQuota>,
    agent_logins: Vec<zz_protocol::AgentLogin>,
    full_pending: HashSet<PaneId>,
    prefix_armed: bool,
    key_table: Option<(String, bool)>,
    command_prompt: Option<CommandPromptState>,
    command_output: Option<(u64, PaneId, RetainedTerminalViewport)>,
    command_output_watermark: u64,
    choose_tree: Option<ChooseTreeState>,
    choose_buffer: Option<ChooseBufferState>,
    chooser_presentation: Option<ChooserPresentation>,
    display_panes: Option<DisplayPanesState>,
    menu: Option<MenuState>,
    menu_opened: u64,
    confirm: Option<ConfirmState>,
    outbound: VecDeque<Outbound>,
    events: VecDeque<CoreEvent>,
    clipboard_query: bool,
}

impl ClientCore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_clipboard_query(&mut self) -> bool {
        std::mem::take(&mut self.clipboard_query)
    }

    pub fn handle_message(&mut self, message: ProtocolMessage) {
        match message {
            ProtocolMessage::ServerHello(hello) => self.reset_connection(*hello),
            ProtocolMessage::Welcome(welcome) => {
                self.clear_attachment();
                self.reset_session();
                self.appearance = None;
                self.mux_options = MuxOptions::default();
                self.refresh_prefix_keys();
                self.key_tables.clear();
                self.key_tables_hash = 0;
                self.mouse_bindings = MouseBindings::default();
                self.status = StatusLine::default();
                self.adopt_welcome(welcome);
                self.events.push_back(CoreEvent::HelloReceived);
            }
            ProtocolMessage::Batch(batch) => {
                let Ok(messages) = batch.messages() else {
                    self.request_tree_sync();
                    return;
                };
                self.begin_event_group();
                for message in messages {
                    self.handle_message(message);
                }
                self.finish_event_group();
            }
            ProtocolMessage::Attached {
                session,
                snapshot,
                read_only,
                client_flags,
            } => {
                self.attached_session = Some(session);
                self.attached_read_only = read_only;
                self.attached_client_flags = client_flags;
                self.tree = snapshot.clone();
                self.snapshot = Arc::new(snapshot);
                self.client_view = ClientView::default();
                self.tree_sync_pending = false;
                self.viewports.clear();
                self.spare_cells.clear();
                self.full_pending.clear();
                let prefix_changed = self.prefix_armed;
                let key_table_changed = self.key_table.is_some();
                let command_prompt_changed = self.command_prompt.is_some();
                let command_output_changed = self.command_output.is_some();
                let choose_tree_changed = self.choose_tree.is_some();
                let choose_buffer_changed = self.choose_buffer.is_some();
                let display_panes_changed = self.display_panes.is_some();
                let menu_changed = self.menu.is_some();
                let confirm_changed = self.confirm.is_some();
                self.reset_session();
                self.events.push_back(CoreEvent::Attached { session });
                self.events.push_back(CoreEvent::SnapshotChanged);
                if prefix_changed {
                    self.events
                        .push_back(CoreEvent::PrefixArmed { armed: false });
                }
                if key_table_changed {
                    self.events.push_back(CoreEvent::KeyTableChanged);
                }
                if command_prompt_changed {
                    self.events.push_back(CoreEvent::CommandPromptChanged);
                }
                if command_output_changed {
                    self.events.push_back(CoreEvent::CommandOutputChanged);
                }
                if choose_tree_changed {
                    self.events.push_back(CoreEvent::ChooseTreeChanged);
                }
                if choose_buffer_changed {
                    self.events.push_back(CoreEvent::ChooseBufferChanged);
                }
                if display_panes_changed {
                    self.events.push_back(CoreEvent::DisplayPanesChanged);
                }
                if menu_changed {
                    self.events.push_back(CoreEvent::MenuChanged);
                }
                if confirm_changed {
                    self.events.push_back(CoreEvent::ConfirmChanged);
                }
            }
            ProtocolMessage::Event(Event {
                sequence: _,
                payload,
            }) => self.handle_payload(payload),
            ProtocolMessage::CommandResponse(response) => {
                self.events.push_back(CoreEvent::CommandResponse(response));
            }
            other => {
                self.events.push_back(CoreEvent::Message(Box::new(other)));
            }
        }
    }

    pub fn poll_outbound(&mut self) -> Option<Outbound> {
        self.outbound.pop_front()
    }

    pub fn poll_event(&mut self) -> Option<CoreEvent> {
        self.events.pop_front()
    }

    pub fn begin_event_group(&mut self) {
        self.batch_reducing = true;
        self.event_group_start = self.events.len();
    }

    pub fn finish_event_group(&mut self) {
        self.batch_reducing = false;
        if self.tree_dirty {
            self.materialize_tree();
        }
        let mut pending = self
            .events
            .split_off(self.event_group_start.min(self.events.len()));
        let mut grouped = VecDeque::new();
        while let Some(event) = pending.pop_front() {
            if matches!(
                event,
                CoreEvent::SnapshotChanged
                    | CoreEvent::StatusChanged
                    | CoreEvent::MuxOptionsChanged
                    | CoreEvent::KeyTablesChanged
            ) && grouped.contains(&event)
            {
                continue;
            }
            grouped.push_back(event);
        }
        self.events.append(&mut grouped);
    }

    #[must_use]
    pub const fn hello_received(&self) -> bool {
        self.hello_received
    }

    #[must_use]
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    #[must_use]
    pub fn appearance(&self) -> Option<&TerminalAppearance> {
        self.appearance.as_deref()
    }

    #[must_use]
    pub const fn appearance_provenance(&self) -> &AppearanceProvenance {
        &self.appearance_provenance
    }

    #[must_use]
    pub const fn mux_options(&self) -> &MuxOptions {
        &self.mux_options
    }

    #[must_use]
    pub fn set_clipboard_writes(&self) -> bool {
        self.mux_options
            .get(zz_protocol::MuxOptionKey::SetClipboard)
            .is_none_or(|option| option.value != "off")
    }

    #[must_use]
    pub fn clipboard_write_lands(
        &self,
        producer: ClipboardProducer,
        target: ClipboardTarget,
    ) -> bool {
        match target {
            ClipboardTarget::Clipboard => true,
            ClipboardTarget::Primary => {
                producer == ClipboardProducer::Server && self.set_clipboard_writes()
            }
        }
    }

    #[must_use]
    pub const fn terminal_negotiation(&self) -> Option<&(Vec<String>, Vec<String>, Vec<String>)> {
        self.terminal_negotiation.as_ref()
    }

    #[must_use]
    pub fn canonical_prefix(&self) -> Option<String> {
        self.mux_options
            .get(zz_protocol::MuxOptionKey::Prefix)
            .map(|option| zz_protocol::canonical_key(&option.value))
    }

    #[must_use]
    pub fn key_tables(&self) -> &[KeyTableSnapshot] {
        &self.key_tables
    }

    #[must_use]
    pub const fn key_tables_hash(&self) -> u64 {
        self.key_tables_hash
    }

    #[must_use]
    pub const fn mouse_bindings(&self) -> &MouseBindings {
        &self.mouse_bindings
    }

    #[must_use]
    pub const fn layout_generation(&self) -> u64 {
        self.client_view.layout_generation
    }

    #[must_use]
    pub const fn server_clock(&self) -> u64 {
        self.server_clock
    }

    #[must_use]
    pub const fn server_id(&self) -> u64 {
        self.server_id
    }

    #[must_use]
    pub const fn server_revision(&self) -> u32 {
        self.server_revision
    }

    #[must_use]
    pub const fn host_platform(&self) -> zz_protocol::HostPlatform {
        self.host_platform
    }

    pub fn adopt_welcome(&mut self, welcome: Welcome) {
        self.hello_received = true;
        self.server_id = welcome.server_id;
        self.server_revision = welcome.revision;
        self.server_clock = welcome.clock;
        self.host_platform = welcome.platform;
        self.capabilities = welcome.capability_strings();
        self.command_output_watermark = 0;
    }

    #[must_use]
    pub fn prefix_bindings(&self) -> &[KeyBindingSnapshot] {
        self.key_tables
            .iter()
            .find(|table| table.name == "prefix")
            .map_or(&[], |table| table.bindings.as_slice())
    }

    #[must_use]
    pub const fn status(&self) -> &StatusLine {
        &self.status
    }

    #[must_use]
    pub const fn snapshot(&self) -> &Arc<MuxSnapshot> {
        &self.snapshot
    }

    #[must_use]
    pub const fn attached_session(&self) -> Option<SessionId> {
        self.attached_session
    }

    #[must_use]
    pub fn modal_capture(&self) -> Option<PaneId> {
        let session = self.attached_session?;
        let session = self
            .snapshot
            .sessions
            .iter()
            .find(|candidate| candidate.id == session)?;
        let window = self.snapshot.focused_window_for(session);
        session
            .windows
            .iter()
            .find(|candidate| candidate.id == window)?
            .modal
            .filter(|modal| modal.capture_keys)
            .map(|modal| modal.pane)
    }

    #[must_use]
    pub const fn attached_read_only(&self) -> bool {
        self.attached_read_only
    }

    #[must_use]
    pub fn attached_client_flags(&self) -> &str {
        &self.attached_client_flags
    }

    #[must_use]
    pub const fn last_detach_reason(&self) -> Option<CoreDetachReason> {
        self.last_detach_reason
    }

    #[must_use]
    pub const fn last_detach_was_session_destroyed(&self) -> bool {
        matches!(
            self.last_detach_reason,
            Some(CoreDetachReason::SessionDestroyed)
        )
    }

    #[must_use]
    pub const fn last_detach_was_server_stopping(&self) -> bool {
        matches!(
            self.last_detach_reason,
            Some(CoreDetachReason::ServerStopping)
        )
    }

    #[must_use]
    pub fn viewport(&self, pane: PaneId) -> Option<&TerminalViewport> {
        self.viewports.get(&pane).map(|retained| &retained.viewport)
    }

    #[must_use]
    pub fn retained_viewport(&self, pane: PaneId) -> Option<&RetainedTerminalViewport> {
        self.viewports.get(&pane)
    }

    #[must_use]
    pub fn copy_generation(&self, pane: PaneId) -> u64 {
        self.copy_generations
            .get(&pane)
            .copied()
            .unwrap_or_default()
    }

    pub fn retain_history(&mut self) {
        self.history_limit = MAX_HISTORY_ROWS;
        for retained in self.viewports.values_mut() {
            if retained.history.limit() != MAX_HISTORY_ROWS {
                retained.history = HistoryRing::with_limit(MAX_HISTORY_ROWS);
            }
        }
    }

    #[must_use]
    pub fn history_trickle_budget(&self) -> usize {
        self.mux_options
            .get(zz_protocol::MuxOptionKey::HistoryTrickle)
            .and_then(|option| option.value.parse::<usize>().ok())
            .unwrap_or_default()
            .min(MAX_HISTORY_ROWS)
    }

    pub fn apply_history_chunk(
        &mut self,
        pane: PaneId,
        start: u32,
        total: u32,
        offset: u32,
        columns: u16,
        rows: Vec<Vec<PackedCell>>,
        dictionary: TerminalDictionary,
    ) -> bool {
        let Some(retained) = self.viewports.get_mut(&pane) else {
            return false;
        };
        let applied = apply_history_chunk(
            retained,
            start,
            total,
            offset,
            columns,
            rows,
            dictionary,
            &mut self.next_row_revision,
        );
        if applied {
            self.events.push_back(CoreEvent::ViewportChanged {
                pane,
                damage: ViewportDamage::Rows(Vec::new()),
            });
        }
        applied
    }

    #[must_use]
    pub fn agent_state(&self, pane: PaneId) -> Option<&AgentPaneWire> {
        self.agent_states.get(&pane)
    }

    #[must_use]
    pub fn simulator(&self, pane: PaneId) -> Option<&SimulatorDescriptor> {
        self.simulators.get(&pane)
    }

    #[must_use]
    pub fn agent_parent(&self, pane: PaneId) -> Option<PaneId> {
        self.agent_parents.get(&pane).copied()
    }

    #[must_use]
    pub const fn providers(&self) -> Option<&zz_protocol::ProvidersReport> {
        self.providers.as_ref()
    }

    #[must_use]
    pub fn agent_accounts(&self) -> Option<&[zz_protocol::AgentAccount]> {
        self.agent_accounts.as_deref()
    }

    #[must_use]
    pub fn agent_quotas(&self) -> &[zz_protocol::AgentQuota] {
        &self.agent_quotas
    }

    #[must_use]
    pub fn agent_logins(&self) -> &[zz_protocol::AgentLogin] {
        &self.agent_logins
    }

    #[must_use]
    pub fn agent_login(
        &self,
        provider: zz_protocol::AgentProvider,
    ) -> Option<&zz_protocol::AgentLogin> {
        self.agent_logins
            .iter()
            .find(|login| login.provider == provider)
    }

    pub fn dismiss_agent_login(&mut self, login: u64) {
        self.agent_logins
            .retain(|known| known.id != login || !known.finished());
    }

    #[must_use]
    pub const fn prefix_armed(&self) -> bool {
        self.prefix_armed
    }

    #[must_use]
    pub fn key_table(&self) -> Option<(&str, bool)> {
        self.key_table
            .as_ref()
            .map(|(table, repeat)| (table.as_str(), *repeat))
    }

    pub fn claims_prefix_input(&self, input: &zz_terminal::KeyInput) -> bool {
        if input.modifiers.platform() {
            return false;
        }
        if self.prefix_armed || self.key_table.is_some() {
            return true;
        }
        self.prefix_keys
            .matches(zz_protocol::input_key_name(input).as_str())
    }

    fn refresh_prefix_keys(&mut self) {
        self.prefix_keys = PrefixKeys::from_options(&self.mux_options);
    }

    #[must_use]
    pub const fn command_prompt(&self) -> Option<&CommandPromptState> {
        self.command_prompt.as_ref()
    }

    #[must_use]
    pub fn command_output(&self) -> Option<(PaneId, &TerminalViewport)> {
        self.command_output
            .as_ref()
            .map(|(_, pane, retained)| (*pane, &retained.viewport))
    }

    #[must_use]
    pub fn retained_command_output(&self) -> Option<(PaneId, &RetainedTerminalViewport)> {
        self.command_output
            .as_ref()
            .map(|(_, pane, retained)| (*pane, retained))
    }

    #[must_use]
    pub fn command_output_id(&self) -> Option<u64> {
        self.command_output
            .as_ref()
            .map(|(output_id, _, _)| *output_id)
    }

    #[must_use]
    pub const fn choose_tree(&self) -> Option<&ChooseTreeState> {
        self.choose_tree.as_ref()
    }

    #[must_use]
    pub const fn choose_buffer(&self) -> Option<&ChooseBufferState> {
        self.choose_buffer.as_ref()
    }

    #[must_use]
    pub const fn chooser_presentation(&self) -> Option<&ChooserPresentation> {
        self.chooser_presentation.as_ref()
    }

    #[must_use]
    pub const fn display_panes(&self) -> Option<&DisplayPanesState> {
        self.display_panes.as_ref()
    }

    #[must_use]
    pub const fn menu(&self) -> Option<&MenuState> {
        self.menu.as_ref()
    }

    #[must_use]
    pub const fn menu_opened(&self) -> u64 {
        self.menu_opened
    }

    #[must_use]
    pub const fn confirm(&self) -> Option<&ConfirmState> {
        self.confirm.as_ref()
    }

    pub fn adopt_hello(&mut self, hello: ServerHello) {
        self.command_output_watermark = 0;
        let ServerHello {
            protocol_version: _,
            server_id,
            client_id: _,
            client_instance_id: _,
            capabilities,
            appearance,
            appearance_provenance,
            mux_options,
            status,
            key_tables,
            clock,
            platform,
            revision,
        } = hello;
        self.hello_received = true;
        self.server_id = server_id;
        self.server_revision = revision;
        self.server_clock = clock;
        self.host_platform = platform;
        self.capabilities = capabilities;
        self.appearance = Some(Box::new(appearance));
        self.appearance_provenance = appearance_provenance;
        self.mux_options = mux_options;
        self.refresh_prefix_keys();
        self.key_tables = key_tables;
        self.refresh_key_tables_metadata();
        self.status = status;
    }

    pub fn reset_session(&mut self) {
        self.prefix_armed = false;
        self.key_table = None;
        self.command_prompt = None;
        self.command_output = None;
        self.choose_tree = None;
        self.choose_buffer = None;
        self.chooser_presentation = None;
        self.display_panes = None;
        self.menu = None;
        self.confirm = None;
        self.agent_states.clear();
    }

    pub fn clear_attachment(&mut self) {
        self.attached_session = None;
        self.attached_read_only = false;
        self.attached_client_flags.clear();
        self.last_detach_reason = None;
        self.snapshot = Arc::new(MuxSnapshot::default());
        self.tree = MuxSnapshot::default();
        self.client_view = ClientView::default();
        self.tree_sync_pending = false;
        self.viewports.clear();
        self.spare_cells.clear();
        self.agent_states.clear();
        self.simulators.clear();
        self.agent_parents.clear();
        self.full_pending.clear();
    }

    fn reset_connection(&mut self, hello: ServerHello) {
        self.adopt_hello(hello);
        self.clear_attachment();
        self.reset_session();
        self.events.push_back(CoreEvent::HelloReceived);
    }

    fn handle_payload(&mut self, payload: EventPayload) {
        match payload {
            EventPayload::Snapshot(snapshot) => {
                if self.client_view.attachment_generation == 0 {
                    self.client_view.focused_window = snapshot.focused_window;
                }
                self.tree = snapshot;
                self.tree_sync_pending = false;
                self.full_pending.clear();
                self.materialize_tree();
            }
            EventPayload::TreeDelta(delta) => {
                if self.tree_sync_pending || delta.apply(&mut self.tree).is_err() {
                    self.request_tree_sync();
                } else {
                    self.materialize_tree();
                }
            }
            EventPayload::ClientView(view) => {
                let changed_attachment = view.session != self.attached_session
                    || view.attachment_generation != self.client_view.attachment_generation;
                self.client_view = view;
                self.attached_session = self.client_view.session;
                self.attached_read_only = self.client_view.read_only;
                self.attached_client_flags
                    .clone_from(&self.client_view.client_flags);
                if changed_attachment {
                    let reset_events = self.reset_session_events();
                    self.viewports.clear();
                    self.spare_cells.clear();
                    self.full_pending.clear();
                    if let Some(session) = self.attached_session {
                        self.events.push_back(CoreEvent::Attached { session });
                    }
                    self.events.extend(reset_events);
                }
                self.materialize_tree();
            }
            EventPayload::AppearanceChanged {
                appearance,
                provenance,
            } => {
                self.appearance = Some(appearance);
                self.appearance_provenance = provenance;
                self.events.push_back(CoreEvent::AppearanceChanged);
            }
            EventPayload::MuxOptionsChanged { options } => {
                self.mux_options = options;
                self.refresh_prefix_keys();
                self.events.push_back(CoreEvent::MuxOptionsChanged);
            }
            EventPayload::MuxOptionsPatched { options } => {
                self.mux_options.merge(options);
                self.refresh_prefix_keys();
                self.events.push_back(CoreEvent::MuxOptionsChanged);
            }
            EventPayload::TerminalNegotiation {
                features,
                user_keys,
                application_escape,
            } => {
                self.terminal_negotiation = Some((features, user_keys, application_escape));
                self.events.push_back(CoreEvent::MuxOptionsChanged);
            }
            EventPayload::StatusChanged { status } => {
                self.status = status;
                self.events.push_back(CoreEvent::StatusChanged);
            }
            EventPayload::KeyTablesChanged { tables } => {
                self.key_tables = tables;
                self.refresh_key_tables_metadata();
                self.events.push_back(CoreEvent::KeyTablesChanged);
            }
            EventPayload::KeyTablesPatched { tables, removed } => {
                self.key_tables
                    .retain(|table| !removed.contains(&table.name));
                for table in tables {
                    if let Some(current) = self
                        .key_tables
                        .iter_mut()
                        .find(|current| current.name == table.name)
                    {
                        *current = table;
                    } else {
                        let at = self
                            .key_tables
                            .partition_point(|current| current.name < table.name);
                        self.key_tables.insert(at, table);
                    }
                }
                self.refresh_key_tables_metadata();
                self.events.push_back(CoreEvent::KeyTablesChanged);
            }
            EventPayload::KeyTablesHashChanged { hash, mouse } => {
                self.key_tables_hash = hash;
                self.mouse_bindings = mouse;
                self.events.push_back(CoreEvent::KeyTablesChanged);
            }
            EventPayload::TerminalViewport { pane, viewport } => {
                self.full_pending.remove(&pane);
                self.spare_cells.remove(&pane);
                if let Some(retained) = self.viewports.get_mut(&pane) {
                    replace_retained_viewport(retained, viewport, &mut self.next_row_revision);
                } else {
                    let mut retained = new_retained_viewport(viewport, &mut self.next_row_revision);
                    retained.history = HistoryRing::with_limit(self.history_limit);
                    self.viewports.insert(pane, retained);
                }
                self.events.push_back(CoreEvent::ViewportChanged {
                    pane,
                    damage: ViewportDamage::All,
                });
            }
            EventPayload::TerminalPatch { pane, patch } => self.apply_patch(pane, patch),
            EventPayload::CommandPrompt { state } => {
                self.command_prompt = state;
                self.events.push_back(CoreEvent::CommandPromptChanged);
            }
            EventPayload::CommandOutput {
                pane,
                output_id,
                viewport,
            } => self.apply_command_output(pane, output_id, viewport),
            EventPayload::ChooseTree { state } => {
                self.choose_tree = state;
                self.chooser_presentation = None;
                self.events.push_back(CoreEvent::ChooseTreeChanged);
            }
            EventPayload::ChooseTreeUpdate { search, selected } => {
                self.update_choose_tree(search, selected);
            }
            EventPayload::ChooseBuffer { state } => {
                self.choose_buffer = state;
                self.chooser_presentation = None;
                self.events.push_back(CoreEvent::ChooseBufferChanged);
            }
            EventPayload::ChooseBufferUpdate { search, selected } => {
                self.update_choose_buffer(search, selected);
            }
            EventPayload::ChooserPresentation { presentation } => {
                self.chooser_presentation = presentation.map(|presentation| *presentation);
                if self.choose_tree.is_some() {
                    self.events.push_back(CoreEvent::ChooseTreeChanged);
                } else if self.choose_buffer.is_some() {
                    self.events.push_back(CoreEvent::ChooseBufferChanged);
                }
            }
            EventPayload::DisplayPanes { state } => {
                self.display_panes = state;
                self.events.push_back(CoreEvent::DisplayPanesChanged);
            }
            EventPayload::Menu { state } => {
                if self.menu.is_none() && state.is_some() {
                    self.menu_opened = self.menu_opened.wrapping_add(1);
                }
                self.menu = state;
                self.events.push_back(CoreEvent::MenuChanged);
            }
            EventPayload::Confirm { state } => {
                self.confirm = state;
                self.events.push_back(CoreEvent::ConfirmChanged);
            }
            EventPayload::PrefixArmed { armed } => {
                self.prefix_armed = armed;
                self.events.push_back(CoreEvent::PrefixArmed { armed });
            }
            EventPayload::PrefixCancelled { request_id } => {
                self.events
                    .push_back(CoreEvent::PrefixCancelled { request_id });
            }
            EventPayload::KeyTableActive { table, repeat } => {
                self.key_table = table.map(|table| (table, repeat));
                self.events.push_back(CoreEvent::KeyTableChanged);
            }
            EventPayload::PaneRemoved(pane) => {
                self.viewports.remove(&pane);
                self.spare_cells.remove(&pane);
                self.copy_generations.remove(&pane);
                self.agent_states.remove(&pane);
                self.simulators.remove(&pane);
                self.full_pending.remove(&pane);
                if self
                    .command_output
                    .as_ref()
                    .is_some_and(|(_, output_pane, _)| *output_pane == pane)
                {
                    self.command_output = None;
                    self.events.push_back(CoreEvent::CommandOutputChanged);
                }
                self.events.push_back(CoreEvent::PaneRemoved { pane });
            }
            EventPayload::Detached {
                session,
                by,
                reason,
                action,
            } => {
                if self.attached_session == Some(session) {
                    self.attached_session = None;
                }
                self.last_detach_reason = Some(if reason.is_requested() {
                    CoreDetachReason::Requested
                } else if reason.is_evicted() {
                    CoreDetachReason::Evicted
                } else if reason.is_session_destroyed() {
                    CoreDetachReason::SessionDestroyed
                } else {
                    debug_assert!(reason.is_server_stopping());
                    CoreDetachReason::ServerStopping
                });
                self.events.push_back(CoreEvent::Detached {
                    session,
                    by,
                    action,
                });
            }
            EventPayload::ServerStopping => self.events.push_back(CoreEvent::ServerStopping),
            EventPayload::Bell { pane } => self.events.push_back(CoreEvent::Bell { pane }),
            EventPayload::FocusSidebar => self.events.push_back(CoreEvent::FocusSidebar),
            EventPayload::OpenPathPicker { pane, start_dir } => {
                self.events
                    .push_back(CoreEvent::OpenPathPicker { pane, start_dir });
            }
            EventPayload::ClientMessage { pane, kind, text } => {
                self.events.push_back(CoreEvent::ClientMessage {
                    pane,
                    kind,
                    text,
                    duration_ms: None,
                    message_id: None,
                });
            }
            EventPayload::TimedClientMessage {
                pane,
                kind,
                text,
                duration_ms,
                message_id,
            } => {
                self.events.push_back(CoreEvent::ClientMessage {
                    pane,
                    kind,
                    text,
                    duration_ms: Some(duration_ms),
                    message_id: Some(message_id),
                });
            }
            EventPayload::TimedClientMessageCleared { message_id } => {
                self.events
                    .push_back(CoreEvent::ClientMessageCleared { message_id });
            }
            EventPayload::Clipboard {
                pane,
                request_id,
                target,
                text,
                producer,
            } => {
                if producer == ClipboardProducer::Server
                    && !text.is_empty()
                    && self.clipboard_write_lands(producer, target)
                {
                    let copies = self.copy_generations.entry(pane).or_default();
                    *copies = copies.wrapping_add(1);
                }
                self.events.push_back(CoreEvent::Clipboard {
                    pane,
                    request_id,
                    target,
                    text,
                    producer,
                });
            }
            EventPayload::OpenUri { pane, uri } => {
                self.events.push_back(CoreEvent::OpenUri { pane, uri });
            }
            EventPayload::BrowserCommand { pane, command } => {
                self.events
                    .push_back(CoreEvent::BrowserCommand { pane, command });
            }
            EventPayload::TerminalUiCommand { pane, command } => {
                self.events
                    .push_back(CoreEvent::TerminalUiCommand { pane, command });
            }
            EventPayload::HistoryChunk {
                pane,
                start,
                total,
                offset,
                columns,
                rows,
                dictionary,
            } => {
                self.events.push_back(CoreEvent::HistoryChunk {
                    pane,
                    start,
                    total,
                    offset,
                    columns,
                    rows,
                    dictionary,
                });
            }
            EventPayload::KittyImageBegin {
                pane,
                image_id,
                generation,
                width,
                height,
                total_bytes,
            } => {
                self.events.push_back(CoreEvent::KittyImageBegin {
                    pane,
                    image_id,
                    generation,
                    width,
                    height,
                    total_bytes,
                });
            }
            EventPayload::KittyImageChunk {
                pane,
                image_id,
                generation,
                bytes,
            } => {
                self.events.push_back(CoreEvent::KittyImageChunk {
                    pane,
                    image_id,
                    generation,
                    bytes,
                });
            }
            EventPayload::KittyImagesRemoved { pane, image_ids } => {
                self.events
                    .push_back(CoreEvent::KittyImagesRemoved { pane, image_ids });
            }
            EventPayload::AgentUpdates {
                pane,
                first_seq,
                items,
            } => {
                self.events.push_back(CoreEvent::AgentUpdates {
                    pane,
                    first_seq,
                    items,
                });
            }
            EventPayload::AgentState { pane, state } => {
                let attention = self
                    .agent_states
                    .get(&pane)
                    .and_then(|previous| agent_attention_edge(previous, &state));
                self.agent_states.insert(pane, state);
                self.events
                    .push_back(CoreEvent::AgentStateChanged { pane, attention });
            }
            EventPayload::AgentLagged { pane, next_seq } => {
                self.events
                    .push_back(CoreEvent::AgentLagged { pane, next_seq });
            }
            EventPayload::AgentReplayed { pane, through_seq } => {
                self.events
                    .push_back(CoreEvent::AgentReplayed { pane, through_seq });
            }
            EventPayload::AgentSessions {
                pane,
                request_id,
                result,
            } => {
                self.events.push_back(CoreEvent::AgentSessions {
                    pane,
                    request_id,
                    result,
                });
            }
            EventPayload::Providers(report) => {
                self.providers = Some(report);
                self.events.push_back(CoreEvent::ProvidersChanged);
            }
            EventPayload::AgentAccounts(accounts) => {
                self.agent_accounts = Some(accounts);
                self.events.push_back(CoreEvent::AgentAccountsChanged);
            }
            EventPayload::AgentQuotas(quotas) => {
                self.agent_quotas = quotas;
                self.events.push_back(CoreEvent::AgentAccountsChanged);
            }
            EventPayload::AgentLogin(login) => {
                let provider = login.provider;
                if !crate::agent_login::reduce(&mut self.agent_logins, login) {
                    return;
                }
                self.events
                    .push_back(CoreEvent::AgentLoginChanged { provider });
            }
            EventPayload::SimulatorPane { pane, descriptor } => {
                if self.simulators.get(&pane) == Some(&descriptor) {
                    return;
                }
                self.simulators.insert(pane, descriptor);
                self.events.push_back(CoreEvent::SimulatorChanged { pane });
            }
            EventPayload::SimulatorCommand { pane, command } => {
                self.events
                    .push_back(CoreEvent::SimulatorCommand { pane, command });
            }
            EventPayload::AgentParents { parents } => self.agent_parents = parents,
            EventPayload::ControlExit { .. }
            | EventPayload::HookEvent { .. }
            | EventPayload::PaneOutput { .. }
            | EventPayload::PaneOutputState { .. }
            | EventPayload::PaneOutputAged { .. }
            | EventPayload::ControlFlags { .. }
            | EventPayload::ControlCommandGuard { .. }
            | EventPayload::ControlCommandGuardRaw { .. }
            | EventPayload::ControlCommandStarted { .. }
            | EventPayload::ControlCommandOutput { .. }
            | EventPayload::ControlConfigError { .. }
            | EventPayload::ControlSourceFile { .. }
            | EventPayload::StartupConfigCauses { .. }
            | EventPayload::CommandStdout { .. }
            | EventPayload::CommandClientExit
            | EventPayload::RetiredPopup { .. }
            | EventPayload::SubscriptionChanged { .. } => {}
            EventPayload::ClipboardQuery => self.clipboard_query = true,
        }
    }

    fn reset_session_events(&mut self) -> Vec<CoreEvent> {
        let events = [
            (self.prefix_armed, CoreEvent::PrefixArmed { armed: false }),
            (self.key_table.is_some(), CoreEvent::KeyTableChanged),
            (
                self.command_prompt.is_some(),
                CoreEvent::CommandPromptChanged,
            ),
            (
                self.command_output.is_some(),
                CoreEvent::CommandOutputChanged,
            ),
            (self.choose_tree.is_some(), CoreEvent::ChooseTreeChanged),
            (self.choose_buffer.is_some(), CoreEvent::ChooseBufferChanged),
            (self.display_panes.is_some(), CoreEvent::DisplayPanesChanged),
            (self.menu.is_some(), CoreEvent::MenuChanged),
            (self.confirm.is_some(), CoreEvent::ConfirmChanged),
        ]
        .into_iter()
        .filter_map(|(changed, event)| changed.then_some(event))
        .collect();
        self.reset_session();
        events
    }

    fn request_tree_sync(&mut self) {
        if !self.tree_sync_pending {
            self.tree_sync_pending = true;
            self.outbound.push_back(Outbound::TreeSync);
        }
    }

    fn materialize_tree(&mut self) {
        self.tree_dirty = true;
        if self.batch_reducing {
            return;
        }
        let Ok(snapshot) = self.client_view.apply_owned(self.tree.clone()) else {
            self.request_tree_sync();
            return;
        };
        self.tree_dirty = false;
        self.snapshot = Arc::new(snapshot);
        self.retain_snapshot_panes();
        self.events.push_back(CoreEvent::SnapshotChanged);
    }

    fn refresh_key_tables_metadata(&mut self) {
        self.key_tables_hash = key_tables_hash(&self.key_tables);
        self.mouse_bindings = MouseBindings::from_tables(&self.key_tables);
    }

    fn apply_patch(&mut self, pane: PaneId, patch: TerminalViewportPatch) {
        let Some(retained) = self.viewports.get_mut(&pane) else {
            self.request_full(pane);
            return;
        };
        let damage = patch_damage(&retained.viewport, &patch);
        let retired = adopt_spare_cells(&mut retained.viewport, self.spare_cells.remove(&pane))
            .filter(|_| patch.scroll == 0)
            .map(|(cells, mut stale)| {
                stale.extend(patch.changed_rows.row_indices());
                (cells, stale)
            });
        if apply_retained_patch(retained, patch, &mut self.next_row_revision).is_ok() {
            let viewport = &retained.viewport;
            if let Some((cells, stale)) = retired
                && !Arc::ptr_eq(&cells, &viewport.cells)
            {
                self.spare_cells.insert(
                    pane,
                    SpareCells {
                        cells,
                        stale,
                        generation: (viewport.generation, viewport.view_generation),
                    },
                );
            }
            self.events
                .push_back(CoreEvent::ViewportChanged { pane, damage });
        } else {
            self.viewports.remove(&pane);
            self.request_full(pane);
        }
    }

    fn apply_command_output(
        &mut self,
        pane: PaneId,
        output_id: u64,
        viewport: Option<TerminalViewport>,
    ) {
        if output_id == 0 {
            if viewport.is_none() {
                self.command_output = None;
                self.events.push_back(CoreEvent::CommandOutputChanged);
            }
            return;
        }
        if output_id < self.command_output_watermark {
            return;
        }

        match viewport {
            Some(viewport) if output_id > self.command_output_watermark => {
                self.command_output_watermark = output_id;
                self.command_output = Some((
                    output_id,
                    pane,
                    new_retained_viewport(viewport, &mut self.next_row_revision),
                ));
                self.events.push_back(CoreEvent::CommandOutputChanged);
            }
            Some(viewport) if self.command_output_id() == Some(output_id) => {
                self.command_output = Some((
                    output_id,
                    pane,
                    new_retained_viewport(viewport, &mut self.next_row_revision),
                ));
                self.events.push_back(CoreEvent::CommandOutputChanged);
            }
            None if output_id > self.command_output_watermark => {
                self.command_output_watermark = output_id;
                self.command_output = None;
                self.events.push_back(CoreEvent::CommandOutputChanged);
            }
            None if self.command_output_id() == Some(output_id) => {
                self.command_output = None;
                self.events.push_back(CoreEvent::CommandOutputChanged);
            }
            Some(_) | None => {}
        }
    }

    fn request_full(&mut self, pane: PaneId) {
        if self.full_pending.insert(pane) {
            self.outbound.push_back(Outbound::RequestFull(pane));
        }
    }

    fn retain_snapshot_panes(&mut self) {
        let live: HashSet<PaneId> = self
            .snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .flat_map(|window| window.panes.keys().copied())
            .collect();
        self.viewports.retain(|pane, _| live.contains(pane));
        self.spare_cells.retain(|pane, _| live.contains(pane));
        self.copy_generations.retain(|pane, _| live.contains(pane));
        self.agent_states.retain(|pane, _| live.contains(pane));
        self.full_pending.retain(|pane| live.contains(pane));
    }

    fn update_choose_tree(&mut self, search: Option<ChooseTreeSearchState>, selected: u32) {
        if let Some(state) = self.choose_tree.as_mut() {
            state.search = search;
            state.selected = clamp_selected(selected, state.items.len());
            self.events.push_back(CoreEvent::ChooseTreeChanged);
        }
    }

    fn update_choose_buffer(&mut self, search: Option<ChooseBufferSearchState>, selected: u32) {
        if let Some(state) = self.choose_buffer.as_mut() {
            state.search = search;
            state.selected = clamp_selected(selected, state.items.len());
            self.events.push_back(CoreEvent::ChooseBufferChanged);
        }
    }
}

fn clamp_selected(selected: u32, items: usize) -> u32 {
    selected.min(u32::try_from(items.saturating_sub(1)).unwrap_or(u32::MAX))
}

#[derive(Debug)]
struct SpareCells {
    cells: Arc<[PackedCell]>,
    stale: Vec<u16>,
    generation: (u64, u64),
}

fn adopt_spare_cells(
    viewport: &mut TerminalViewport,
    spare: Option<SpareCells>,
) -> Option<(Arc<[PackedCell]>, Vec<u16>)> {
    if Arc::get_mut(&mut viewport.cells).is_some() {
        return None;
    }
    let retired = Arc::clone(&viewport.cells);
    let Some(SpareCells {
        mut cells,
        mut stale,
        generation,
    }) = spare
    else {
        return Some((retired, Vec::new()));
    };
    if generation == (viewport.generation, viewport.view_generation)
        && cells.len() == retired.len()
        && let Some(buffer) = Arc::get_mut(&mut cells)
    {
        let columns = usize::from(viewport.columns);
        for row in stale.iter().map(|row| usize::from(*row) * columns) {
            if let (Some(target), Some(source)) = (
                buffer.get_mut(row..row + columns),
                retired.get(row..row + columns),
            ) {
                target.copy_from_slice(source);
            }
        }
        viewport.cells = cells;
    }
    stale.clear();
    Some((retired, stale))
}

fn patch_damage(previous: &TerminalViewport, patch: &TerminalViewportPatch) -> ViewportDamage {
    if patch.scroll != 0
        || patch.carries(TerminalPatchFields::COLORS)
            && (patch.foreground != previous.foreground || patch.background != previous.background)
    {
        return ViewportDamage::All;
    }
    let mut rows = patch.changed_rows.row_indices().collect::<Vec<_>>();
    if patch.carries(TerminalPatchFields::OVERLAYS) {
        rows.extend(previous.overlays.iter().map(|overlay| overlay.row));
        rows.extend(patch.overlays.iter().map(|overlay| overlay.row));
        rows.sort_unstable();
        rows.dedup();
    }
    ViewportDamage::Rows(rows)
}

#[cfg(test)]
mod agent_tests;

#[cfg(test)]
mod attachment_tests;

#[cfg(test)]
mod command_output_tests;

#[cfg(test)]
mod key_table_tests;

#[cfg(test)]
mod modal_tests;

#[cfg(test)]
mod mux_options_tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tree_tests;
