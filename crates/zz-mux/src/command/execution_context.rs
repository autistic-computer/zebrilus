use super::*;

fn mouse_anchor(mouse: &MouseEventTarget) -> (u16, u16) {
    mouse
        .drag
        .map(|drag| drag.anchor)
        .or(mouse.press)
        .unwrap_or((mouse.column, mouse.row))
}

pub(super) fn mouse_anchor_cell(mouse: &MouseEventTarget) -> (i32, i32) {
    let (x, y) = mouse_anchor(mouse);
    (i32::from(x), i32::from(y))
}

pub(super) fn mouse_last_cell(mouse: &MouseEventTarget) -> (i32, i32) {
    let (x, y) = mouse
        .drag
        .map(|drag| drag.last)
        .or(mouse.press)
        .unwrap_or((mouse.column, mouse.row));
    (i32::from(x), i32::from(y))
}

pub(super) fn armed_mouse_drag(
    kind: MouseDragKind,
    pane: PaneId,
    mouse: &MouseEventTarget,
) -> MuxEffect {
    MuxEffect::ArmMouseDrag(MouseDrag {
        kind,
        pane,
        anchor: mouse_anchor(mouse),
        last: (mouse.column, mouse.row),
    })
}

impl MouseEventTarget {
    #[must_use]
    pub fn target_spelling(&self) -> Option<String> {
        self.pane
            .map(|pane| pane.to_string())
            .or_else(|| self.window.map(|window| window.to_string()))
    }
}

#[must_use]
pub fn resolve_invoking_mouse_targets(
    command: &CommandInvocation,
    mouse: &MouseEventTarget,
) -> Option<CommandInvocation> {
    mouse
        .target_spelling()
        .and_then(|target| resolve_mouse_targets(command, &target))
}

pub(super) fn resolve_mouse_targets(
    command: &CommandInvocation,
    target: &str,
) -> Option<CommandInvocation> {
    let is_mouse_slot = |command: &CommandInvocation, index: usize| {
        index > 0
            && matches!(command.args[index - 1].as_str(), "-t" | "-s")
            && !command.argument_is_command_block(index)
            && matches!(command.args[index].as_str(), "=" | "{mouse}")
    };
    if !(0..command.args.len()).any(|index| is_mouse_slot(command, index)) {
        return None;
    }
    let mut resolved = command.clone();
    for index in 0..resolved.args.len() {
        if is_mouse_slot(&resolved, index) {
            resolved.args[index] = RawText::from(target);
        }
    }
    Some(resolved)
}

impl fmt::Debug for ExecutionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionContext")
            .field("session", &self.session)
            .field("window", &self.window)
            .field("pane", &self.pane)
            .field("client_terminal", &self.client_terminal)
            .field("client_size", &self.client_size)
            .field("client_working_directory", &self.client_working_directory)
            .field(
                "client_environment_entries",
                &self
                    .client_environment
                    .as_ref()
                    .map(|environment| environment.entries().count()),
            )
            .field("client_attached", &self.client_attached)
            .field("client_attached_context", &self.client_attached_context)
            .field(
                "target_format_client_override",
                &self.target_format_client_override,
            )
            .field("repeat_binding", &self.repeat_binding)
            .field("invoking_key", &self.invoking_key)
            .field("invoking_mouse", &self.invoking_mouse)
            .field("replay_client", &self.replay_client)
            .field("control_command_target", &self.control_command_target)
            .field("legacy_layouts", &self.legacy_layouts)
            .field("refuse_new_session_attach", &self.refuse_new_session_attach)
            .field("spawns_popup", &self.spawns_popup)
            .field("no_hooks", &self.no_hooks)
            .field("format_variables", &self.format_variables)
            .field("spawn_pane_options", &self.spawn_pane_options)
            .field("agent_parent", &self.agent_parent)
            .finish()
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            session: None,
            window: None,
            pane: None,
            client_terminal: ClientTerminal::Present,
            client_size: None,
            client_working_directory: None,
            client_environment: Some(Arc::default()),
            client_attached: true,
            client_attached_context: None,
            target_format_client_override: None,
            repeat_binding: false,
            invoking_key: None,
            invoking_mouse: None,
            replay_client: None,
            control_command_target: None,
            legacy_layouts: false,
            refuse_new_session_attach: false,
            spawns_popup: false,
            no_hooks: false,
            format_variables: BTreeMap::new(),
            spawn_pane_options: Vec::new(),
            agent_parent: None,
        }
    }
}

impl ExecutionContext {
    #[must_use]
    pub fn new(session: Option<SessionId>, window: Option<WindowId>, pane: Option<PaneId>) -> Self {
        Self {
            session,
            window,
            pane,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn for_pane(state: &MuxState, pane: PaneId) -> Option<Self> {
        let window = state.window_for_pane(pane)?;
        let session = state.windows.get(&window)?.session;
        Some(Self::new(Some(session), Some(window), Some(pane)))
    }

    pub fn retarget(&mut self, target: &Self) {
        self.session = target.session;
        self.window = target.window;
        self.pane = target.pane;
    }

    #[must_use]
    pub fn has_client_terminal(&self) -> bool {
        self.client_terminal == ClientTerminal::Present
    }

    #[must_use]
    pub fn has_no_client(&self) -> bool {
        self.client_terminal == ClientTerminal::NoClient
    }

    pub fn set_client_terminal(&mut self, present: bool) {
        self.client_terminal = if present {
            ClientTerminal::Present
        } else {
            ClientTerminal::Absent
        };
    }

    pub fn set_no_client(&mut self) {
        self.client_terminal = ClientTerminal::NoClient;
        self.client_environment = None;
        self.client_attached = false;
        self.client_attached_context = None;
        self.target_format_client_override = None;
    }

    pub fn set_client_attached(&mut self, attached: bool) {
        self.client_attached = attached;
        self.client_attached_context = None;
        self.target_format_client_override = None;
    }

    #[must_use]
    pub fn attached_client_context(
        &self,
    ) -> Option<(Option<SessionId>, Option<WindowId>, Option<PaneId>)> {
        self.client_attached.then(|| {
            self.client_attached_context.map_or(
                (self.session, self.window, self.pane),
                |(session, window, pane)| (Some(session), Some(window), Some(pane)),
            )
        })
    }

    pub fn set_attached_client_context(&mut self, context: Option<(SessionId, WindowId, PaneId)>) {
        self.client_attached = context.is_some();
        self.client_attached_context = context;
        self.target_format_client_override = None;
    }

    pub fn copy_client_attachment(&mut self, source: &Self) {
        self.client_attached = source.client_attached;
        self.client_attached_context = source.client_attached_context;
        self.target_format_client_override = source.target_format_client_override;
    }

    pub fn set_format_client_session(&mut self, session: Option<SessionId>) {
        self.target_format_client_override =
            Some(session.map_or(FormatClient::NoClient, FormatClient::Attached));
    }

    pub fn set_format_client(&mut self, format_client: FormatClient) {
        self.target_format_client_override = Some(format_client);
    }

    #[must_use]
    pub fn format_client(&self) -> FormatClient {
        if self.has_no_client() {
            FormatClient::NoClient
        } else {
            self.attached_client_context()
                .and_then(|(session, _, _)| session)
                .map_or(FormatClient::Unattached, FormatClient::Attached)
        }
    }

    #[must_use]
    pub fn target_format_client(&self) -> FormatClient {
        self.target_format_client_override
            .unwrap_or_else(|| self.format_client())
    }

    #[must_use]
    pub fn client_size(&self) -> Option<(u16, u16)> {
        self.client_size
    }

    pub fn set_client_size(&mut self, size: Option<(u16, u16)>) {
        self.client_size = size;
    }

    #[must_use]
    pub fn client_working_directory(&self) -> Option<&Path> {
        self.client_working_directory.as_deref()
    }

    pub fn set_client_working_directory(&mut self, working_directory: Option<PathBuf>) {
        self.client_working_directory = working_directory;
    }

    #[must_use]
    pub fn client_environment(&self) -> Option<&BTreeMap<RawText, RawText>> {
        self.client_environment
            .as_deref()
            .map(ClientEnvironmentBlob::map)
    }

    pub fn set_client_environment(&mut self, environment: Option<Arc<ClientEnvironmentBlob>>) {
        self.client_environment = environment;
    }

    #[must_use]
    pub fn repeat_binding(&self) -> bool {
        self.repeat_binding
    }

    pub fn set_repeat_binding(&mut self, repeat: bool) {
        self.repeat_binding = repeat;
    }

    #[must_use]
    pub fn invoking_key(&self) -> Option<&str> {
        self.invoking_key.as_deref()
    }

    pub fn set_invoking_key(&mut self, key: Option<String>) {
        self.invoking_key = key;
    }

    #[must_use]
    pub fn invoking_mouse(&self) -> Option<&MouseEventTarget> {
        self.invoking_mouse.as_ref()
    }

    pub fn set_invoking_mouse(&mut self, mouse: Option<MouseEventTarget>) {
        self.invoking_mouse = mouse;
    }

    #[must_use]
    pub fn replay_client(&self) -> Option<ClientId> {
        self.replay_client
    }

    pub fn set_replay_client(&mut self, client: Option<ClientId>) {
        self.replay_client = client;
    }

    #[must_use]
    pub const fn agent_parent(&self) -> Option<PaneId> {
        self.agent_parent
    }

    pub const fn set_agent_parent(&mut self, parent: Option<PaneId>) {
        self.agent_parent = parent;
    }

    #[must_use]
    pub fn control_command_target(&self) -> Option<(ClientId, u8)> {
        self.control_command_target
    }

    pub fn set_control_command_target(&mut self, target: Option<(ClientId, u8)>) {
        self.control_command_target = target;
    }

    #[must_use]
    pub fn legacy_layouts(&self) -> bool {
        self.legacy_layouts
    }

    pub fn set_legacy_layouts(&mut self, legacy: bool) {
        self.legacy_layouts = legacy;
    }

    #[must_use]
    pub const fn refuses_new_session_attach(&self) -> bool {
        self.refuse_new_session_attach
    }

    pub fn set_refuse_new_session_attach(&mut self, refuse: bool) {
        self.refuse_new_session_attach = refuse;
    }

    #[must_use]
    pub const fn spawns_popup(&self) -> bool {
        self.spawns_popup
    }

    pub fn set_spawns_popup(&mut self, popup: bool) {
        self.spawns_popup = popup;
    }

    pub fn set_spawn_pane_options(&mut self, options: Vec<(String, String)>) {
        self.spawn_pane_options = options;
    }

    pub fn retarget_to_pane(&mut self, state: &MuxState, pane: PaneId) -> bool {
        let Some(target) = Self::for_pane(state, pane) else {
            return false;
        };
        self.retarget(&target);
        true
    }

    pub fn enter_hook(&mut self, variables: BTreeMap<String, String>) {
        self.no_hooks = true;
        self.format_variables = variables;
    }

    #[must_use]
    pub fn format_variables(&self) -> Option<&BTreeMap<String, String>> {
        (!self.format_variables.is_empty()).then_some(&self.format_variables)
    }
}
