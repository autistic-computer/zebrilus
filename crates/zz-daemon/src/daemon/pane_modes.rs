use super::*;

impl Shared {
    pub(super) fn refresh_modes(&self, clients: &BTreeSet<ClientId>) {
        let updates = {
            let inner = self.inner.lock();
            let clients = status_targets(&inner, None, Some(clients));
            if clients.is_empty() {
                return;
            }
            let copy_modes = Arc::new(copy_mode_format_facts(&inner));
            clients
                .iter()
                .filter_map(|client| {
                    let session = client_attached_session(&inner, *client)?;
                    let facts = FormatHookFacts {
                        copy_modes: Arc::clone(&copy_modes),
                        client: Some(client_format_facts(&inner, *client, session)),
                        ..FormatHookFacts::default()
                    };
                    let requests = mode_requests(&inner, *client, session, FormatNeeds::NONE);
                    Some((
                        *client,
                        crate::status::expand_modes(&requests, &facts, &inner.engine),
                    ))
                })
                .collect::<Vec<_>>()
        };
        for (client, modes) in updates {
            let status = self.status.lock().republish_modes(client, modes);
            if let Some(status) = status {
                self.publish_to_client(client, EventPayload::StatusChanged { status });
            }
        }
    }

    pub(super) fn pane_mode_key(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        pane: PaneId,
        input: &zz_terminal::KeyInput,
        text_follows: bool,
    ) -> bool {
        if !self.pane_mode_key_name(client, context, pane, &input_key_name(input)) {
            return false;
        }
        self.suppress_committed_character(
            client,
            pane,
            CommittedTextLane::Terminal,
            input,
            text_follows,
        );
        true
    }

    pub(super) fn pane_mode_key_name(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        pane: PaneId,
        key: &str,
    ) -> bool {
        self.pane_mode_input(client, context, pane, PaneModeInput::Key(key))
    }

    pub(super) fn pane_mode_mouse(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        pane: PaneId,
        key: &str,
        x: usize,
        y: usize,
    ) -> bool {
        self.pane_mode_input(client, context, pane, PaneModeInput::Pointer { key, x, y })
    }

    pub(super) fn pane_mode_input(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        pane: PaneId,
        input: PaneModeInput<'_>,
    ) -> bool {
        let handled = self.pane_mode_input_inner(client, context, pane, input);
        let events = take_pane_mode_hook_events(&mut self.inner.lock());
        self.run_event_hooks(events);
        handled
    }

    fn pane_mode_input_inner(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        pane: PaneId,
        input: PaneModeInput<'_>,
    ) -> bool {
        let Some(mode) = self
            .inner
            .lock()
            .modes
            .pane_modes
            .get(&pane)
            .and_then(|modes| modes.last())
            .cloned()
        else {
            return false;
        };
        match mode {
            PaneModeRequest::Customize(mut mode) => {
                if matches!(input, PaneModeInput::Key(_)) {
                    self.ensure_prompt_history();
                }
                let result = {
                    let inner = self.inner.lock();
                    let facts = borrowed_format_hook_facts(&inner);
                    let mut expand = customize_expander(&inner, pane, &facts);
                    match input {
                        PaneModeInput::Key(key) => {
                            let history = zz_mux::PromptHistories {
                                command: &inner.history.command_history,
                                search: &inner.history.search_history,
                            };
                            inner
                                .engine
                                .customize_key(pane, &mut mode, key, history, &mut expand)
                        }
                        PaneModeInput::Pointer { key, x, y } => {
                            inner
                                .engine
                                .customize_mouse(pane, &mut mode, key, x, y, &mut expand)
                        }
                    }
                };
                if let PaneModeInput::Pointer { x, y, .. } = input
                    && let Some(menu) = &result.menu
                {
                    self.raise_mode_tree_menu(client, pane, menu, x, y);
                }
                self.apply_customize_result(client, context, pane, mode, &result);
                if let Some(edit) = result.edit {
                    let value = edit.value.clone();
                    self.spawn_editor(
                        client,
                        pane,
                        value.as_bytes(),
                        EditorTarget::Option { pane, edit },
                    );
                }
                true
            }
            PaneModeRequest::Switch(mut mode) => {
                let (action, entry) = {
                    let inner = self.inner.lock();
                    let entries = chooser_presentation::switch_matches(&inner, pane, &mode);
                    let (columns, rows) = inner.engine.pane_geometry(pane).unwrap_or((80, 24));
                    let visible = usize::from(rows).saturating_sub(1);
                    let action = match input {
                        PaneModeInput::Key(key) => mode.key(key, entries.len(), visible),
                        PaneModeInput::Pointer { key, x, y } => mode.mouse(
                            key,
                            x,
                            y,
                            entries.len(),
                            visible,
                            usize::from(columns),
                            usize::from(rows),
                        ),
                    };
                    let entry = (action == SwitchAction::Run)
                        .then(|| entries.into_iter().nth(mode.current))
                        .flatten();
                    (action, entry)
                };
                if action == SwitchAction::Run && entry.is_none() {
                    return true;
                }
                if action == SwitchAction::Redraw {
                    let mut inner = self.inner.lock();
                    if let Some(modes) = inner.modes.pane_modes.get_mut(&pane)
                        && matches!(modes.last(), Some(PaneModeRequest::Switch(_)))
                    {
                        modes.pop();
                        modes.push(PaneModeRequest::Switch(mode));
                    }
                    drop(inner);
                    self.publish_mux_snapshots();
                    return true;
                }
                pop_pane_mode(&mut self.inner.lock(), pane);
                let events = take_pane_mode_hook_events(&mut self.inner.lock());
                self.run_event_hooks(events);
                self.reap_pane_mode_kill_panes(client, ClientKind::Interactive, context);
                self.publish_snapshot();
                if let Some(entry) = entry {
                    let template = mode.template.as_deref().unwrap_or("switch-client -Zt '%%'");
                    let target_client =
                        current_format_client(&self.inner.lock(), client).unwrap_or(client);
                    let mut selected_context = context.clone();
                    selected_context.retarget(&entry.context);
                    self.execute_chooser_command(
                        target_client,
                        &mut selected_context,
                        template,
                        &entry.target,
                        "switch-mode",
                        None,
                    );
                }
                true
            }
            PaneModeRequest::Clock => {
                pop_pane_mode(&mut self.inner.lock(), pane);
                self.nudge_client_timers();
                self.reap_pane_mode_kill_panes(client, ClientKind::Interactive, context);
                self.publish_snapshot();
                true
            }
            PaneModeRequest::Panes(mode) => {
                let chosen = {
                    let inner = self.inner.lock();
                    match input {
                        PaneModeInput::Key("Escape" | "q") => Some(None),
                        PaneModeInput::Key(_) | PaneModeInput::Pointer { .. }
                            if mode.ignore_keys =>
                        {
                            None
                        }
                        PaneModeInput::Key(key) => {
                            Some(panes_mode_key_index(key).and_then(|index| {
                                inner.engine.pane_at_index(mode.source_window, index)
                            }))
                        }
                        PaneModeInput::Pointer { key, x, y } => (key == "MouseDown1Pane")
                            .then(|| panes_mode_hit(&inner, pane, &mode, x, y))
                            .flatten()
                            .map(Some),
                    }
                };
                let Some(chosen) = chosen else {
                    return true;
                };
                self.finish_panes_mode(client, context, pane, &mode, chosen);
                true
            }
        }
    }

    pub(super) fn settle_pane_modes(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
    ) {
        let events = take_pane_mode_hook_events(&mut self.inner.lock());
        self.run_event_hooks(events);
        self.reap_pane_mode_kill_panes(client, ClientKind::Interactive, context);
    }

    pub(super) fn apply_customize_result(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        pane: PaneId,
        mode: Box<CustomizeMode>,
        result: &CustomizeResult,
    ) {
        {
            let mut inner = self.inner.lock();
            if let Some(modes) = inner.modes.pane_modes.get_mut(&pane) {
                modes.pop();
                modes.push(PaneModeRequest::Customize(mode));
            }
        }
        if let Some((prompt_type, input)) = &result.remembered {
            self.record_prompt_history(*prompt_type, input);
        }
        if let Some(message) = &result.message {
            self.publish_status_message(client, context, message.clone());
        }
        for command in &result.commands {
            if let Err(error) = self.execute(client, ClientKind::Interactive, context, command) {
                let mut message = daemon_error_text(&error);
                uppercase_first(&mut message);
                self.publish_status_message(client, context, message);
                if result.stop_on_error {
                    break;
                }
            }
        }
        {
            let mut inner = self.inner.lock();
            let mut current = match inner
                .modes
                .pane_modes
                .get(&pane)
                .and_then(|modes| modes.last())
            {
                Some(PaneModeRequest::Customize(mode)) => Some(mode.clone()),
                _ => None,
            };
            if let Some(mode) = current.as_mut() {
                let facts = borrowed_format_hook_facts(&inner);
                let mut expand = customize_expander(&inner, pane, &facts);
                inner.engine.customize_finish(pane, mode, &mut expand);
            }
            if let (Some(mode), Some(modes)) = (current, inner.modes.pane_modes.get_mut(&pane))
                && matches!(modes.last(), Some(PaneModeRequest::Customize(_)))
            {
                modes.pop();
                modes.push(PaneModeRequest::Customize(mode));
            }
        }
        if result.close {
            pop_pane_mode(&mut self.inner.lock(), pane);
            self.reap_pane_mode_kill_panes(client, ClientKind::Interactive, context);
            self.publish_snapshot();
        }
        let events = take_pane_mode_hook_events(&mut self.inner.lock());
        self.run_event_hooks(events);
        self.publish_mux_snapshots();
    }

    pub(super) fn mode_tree_menu_choice(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        owner: &ModeTreeMenu,
        index: u32,
    ) {
        let Some(feed) = usize::try_from(index)
            .ok()
            .and_then(|index| customize_menu_feed(owner.outside, index))
        else {
            return;
        };
        let Some(PaneModeRequest::Customize(mut mode)) = self
            .inner
            .lock()
            .modes
            .pane_modes
            .get(&owner.pane)
            .and_then(|modes| modes.last())
            .cloned()
        else {
            return;
        };
        let result = {
            let inner = self.inner.lock();
            let facts = borrowed_format_hook_facts(&inner);
            let mut expand = customize_expander(&inner, owner.pane, &facts);
            inner
                .engine
                .customize_menu_choice(owner.pane, &mut mode, owner.line, feed, &mut expand)
        };
        let mut context = context.clone();
        self.apply_customize_result(client, &mut context, owner.pane, mode, &result);
    }

    pub(super) fn reap_pane_mode_kill_panes(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
    ) {
        let panes = std::mem::take(&mut self.inner.lock().modes.pane_mode_kill_panes);
        let terminal = client_terminal(&self.inner.lock(), client, kind);
        for pane in panes {
            let target = pane.to_string();
            if let Err(error) = self.execute_with_mux_source_raw(
                client,
                kind,
                context,
                &CommandInvocation::new("kill-pane", ["-t", target.as_str()]),
                MuxOptionSource::RuntimeCommand,
                terminal,
                None,
                None,
            ) {
                log::debug!(
                    target: "zz_daemon::diagnostics::pane_mode",
                    "failed to kill mode source pane={pane}: {error}"
                );
            }
        }
    }

    pub(super) fn arm_copy_mode_refresh(self: &Arc<Self>) {
        self.nudge_client_timers();
    }

    pub(super) fn arm_pane_mode_clock(self: &Arc<Self>) {
        self.nudge_client_timers();
    }

    pub(super) fn reap_copy_mode_kill_panes(self: &Arc<Self>) {
        let panes = std::mem::take(&mut self.inner.lock().modes.copy_kill_panes);
        for pane in panes {
            let Some(mut context) = ({
                let inner = self.inner.lock();
                ExecutionContext::for_pane(&inner.engine.state, pane)
            }) else {
                continue;
            };
            let target = pane.to_string();
            if let Err(error) = self.execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("kill-pane", ["-t", target.as_str()]),
            ) {
                log::debug!(
                    target: "zz_daemon::diagnostics::terminal",
                    "failed to kill copy-mode -k pane={pane}: {error}"
                );
            }
        }
    }

    pub(super) fn inject_pane_mode_keys(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        pane: PaneId,
        keys: &[zz_protocol::KeyToken],
        repeat: u32,
    ) -> Result<(), DaemonError> {
        let keys = keys
            .iter()
            .flat_map(|key| match key {
                zz_protocol::KeyToken::Literal(text) => text
                    .chars()
                    .map(|character| zz_protocol::KeyToken::Literal(character.to_string()))
                    .collect::<Vec<_>>(),
                key => vec![key.clone()],
            })
            .collect::<Vec<_>>();
        for _ in 0..repeat {
            for key in &keys {
                let name = match key {
                    zz_protocol::KeyToken::Named(name) | zz_protocol::KeyToken::Literal(name) => {
                        name.clone()
                    }
                    zz_protocol::KeyToken::Raw(byte) => char::from(*byte).to_string(),
                };
                if self.pane_mode_key_name(client, context, pane, &name) {
                    continue;
                }
                let owners = copy_mode_key_owners(&self.inner.lock(), client, pane);
                if !owners.is_empty() {
                    self.inject_mode_table_keys(&owners, pane, std::slice::from_ref(key), 1);
                    continue;
                }
                let sinks = resolve_input_sinks(&self.inner.lock(), pane)?;
                for sink in sinks {
                    match sink {
                        PaneSink::Terminal(terminal) => {
                            DeferredTerminalCommand::SendTokens {
                                terminals: vec![terminal],
                                keys: vec![key.clone()],
                                repeat: 1,
                            }
                            .run();
                        }
                        PaneSink::Browser(target) => self.publish_for_pane(
                            target,
                            &EventPayload::BrowserCommand {
                                pane: target,
                                command: BrowserCommand::SendKeys(vec![key.clone()]),
                            },
                        ),
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn inject_mode_table_keys(
        self: &Arc<Self>,
        owners: &[ClientId],
        pane: PaneId,
        keys: &[zz_protocol::KeyToken],
        repeat: u32,
    ) {
        let inputs = keys.iter().flat_map(client_key_inputs).collect::<Vec<_>>();
        if inputs.is_empty() {
            return;
        }
        let entered = {
            let mut item = self.command_item.as_ref().expect("command item").lock();
            let entered = item.client_key_injection_depth;
            if entered < MAX_CLIENT_KEY_INJECTION_DEPTH {
                item.client_key_injection_depth += 1;
            }
            entered
        };
        if entered >= MAX_CLIENT_KEY_INJECTION_DEPTH {
            log::warn!(
                target: "zz_daemon::diagnostics::input",
                "send-keys into copy mode stopped at injection depth {entered} for pane={pane}"
            );
            return;
        }
        for owner in owners {
            for _ in 0..repeat {
                for input in &inputs {
                    let key = prompt_key_spelling(input);
                    let Some((kind, commands, mut context)) = ({
                        let inner = self.inner.lock();
                        inner
                            .client(*owner)
                            .and_then(|c| c.copy_session.as_ref())
                            .filter(|session| session.pane == pane && !session.exiting)
                            .and_then(|_| inner.engine.copy_mode_table_for_pane(pane).ok())
                            .and_then(|table| inner.engine.keys.get(table, &key))
                            .map(|binding| binding.commands.clone())
                            .zip(ExecutionContext::for_pane(&inner.engine.state, pane))
                            .map(|(commands, mut context)| {
                                if let Some(registered) = inner.client(*owner) {
                                    registered.seed_context_environment(&mut context);
                                }
                                let kind = inner
                                    .client(*owner)
                                    .and_then(|c| c.kind)
                                    .unwrap_or(ClientKind::Interactive);
                                (kind, commands, context)
                            })
                    }) else {
                        continue;
                    };
                    context.set_invoking_key(Some(key));
                    if let Err(error) = self.execute_key_commands(
                        *owner,
                        kind,
                        &mut context,
                        pane,
                        &commands,
                        false,
                    ) {
                        log::warn!(
                            target: "zz_daemon::diagnostics::input",
                            "send-keys into copy mode failed client={owner} pane={pane} error={error}"
                        );
                    }
                }
            }
        }
        let mut item = self.command_item.as_ref().expect("command item").lock();
        item.client_key_injection_depth = item.client_key_injection_depth.saturating_sub(1);
    }
}

#[derive(Clone, Copy)]
pub(super) enum PaneModeInput<'a> {
    Key(&'a str),
    Pointer { key: &'a str, x: usize, y: usize },
}

fn panes_mode_key_index(key: &str) -> Option<u32> {
    let mut characters = key.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    match character {
        '0'..='9' => Some(u32::from(character) - u32::from('0')),
        'a'..='z' => Some(10 + u32::from(character) - u32::from('a')),
        _ => None,
    }
}

fn panes_mode_hit(
    inner: &ServerState,
    pane: PaneId,
    mode: &PanesMode,
    x: usize,
    y: usize,
) -> Option<PaneId> {
    let (columns, rows) = inner.engine.pane_geometry(pane)?;
    let geometry = inner
        .engine
        .panes_mode_geometry(mode.source_window, columns, rows)?;
    geometry.areas.iter().rev().find_map(|area| {
        let inside = x >= usize::from(area.x)
            && x < usize::from(area.x) + usize::from(area.width)
            && y >= usize::from(area.y)
            && y < usize::from(area.y) + usize::from(area.height);
        inside.then_some(area.pane)
    })
}

pub(super) fn hold_chooser_zoom(
    inner: &mut ServerState,
    client: ClientId,
    pane: PaneId,
    zoom: bool,
) -> bool {
    if !zoom || inner.client(client).is_some_and(|c| c.native_chooser) {
        return false;
    }
    let Some(window) = inner.engine.state.window_for_pane(pane) else {
        return false;
    };
    if inner.engine.state.windows[&window].zoomed_pane.is_some() {
        return false;
    }
    if inner.engine.state.toggle_zoom(pane).is_err() {
        return false;
    }
    inner
        .client_entry(client)
        .chooser_zoom
        .get_or_insert(window);
    inner.engine.state.windows[&window].zoomed_pane.is_some()
}

pub(super) const CLIENT_MODE_DEFAULT_COMMAND: &str = "detach-client -t '%%'";

pub(super) fn native_copy_search_direction(
    steps: &[CommandPromptStep],
    template: Option<&CommandPromptTemplate>,
    prompt_type: CommandPromptType,
    mode: CommandPromptMode,
) -> Option<zz_terminal::SearchDirection> {
    if prompt_type != CommandPromptType::Search || steps.len() != 1 || !steps[0].input.is_empty() {
        return None;
    }
    let CommandPromptTemplate::Commands(commands) = template? else {
        return None;
    };
    let [command] = commands.as_slice() else {
        return None;
    };
    let [flag, action, separator, value] = command.args.as_slice() else {
        return None;
    };
    if command.name != "send-keys" || flag != "-X" || separator != "--" || value != "%%" {
        return None;
    }
    match (mode, action.as_str()) {
        (CommandPromptMode::Text, "search-forward")
        | (CommandPromptMode::Incremental, "search-forward-incremental") => {
            Some(zz_terminal::SearchDirection::Forward)
        }
        (CommandPromptMode::Text, "search-backward")
        | (CommandPromptMode::Incremental, "search-backward-incremental") => {
            Some(zz_terminal::SearchDirection::Backward)
        }
        _ => None,
    }
}

pub(super) fn pane_carries_a_mode_command(
    inner: &ServerState,
    client: ClientId,
    pane: PaneId,
) -> bool {
    inner
        .client(client)
        .is_some_and(|c| c.command_output.is_some())
        || inner
            .clients
            .values()
            .filter_map(|c| c.copy_session.as_ref())
            .any(|session| session.pane == pane && !session.exiting)
}

pub(super) fn counted_copy_mode_action(
    inner: &mut ServerState,
    client: ClientId,
    action: &zz_terminal::TerminalViewAction,
) -> zz_terminal::TerminalViewAction {
    let pending = match action {
        zz_terminal::TerminalViewAction::CopyMode(_)
        | zz_terminal::TerminalViewAction::CopyModeCounted { .. } => inner
            .client_mut(client)
            .and_then(|c| c.key_engine.as_mut())
            .and_then(KeyEngine::take_repeat_count),
        _ => return action.clone(),
    };
    match (pending, action) {
        (Some(count), zz_terminal::TerminalViewAction::CopyMode(inner_action))
            if count > 1
                && inner_action.count_policy() != zz_terminal::CopyModeCountPolicy::Once =>
        {
            zz_terminal::TerminalViewAction::CopyModeCounted {
                action: inner_action.clone(),
                count,
            }
        }
        _ => action.clone(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyModeRefreshRequest {
    On,
    Off,
    Toggle,
}

impl CopyModeRefreshRequest {
    pub(super) const fn applied(self, current: bool) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Toggle => !current,
        }
    }
}

pub(super) fn terminal_view_action_refresh_request(
    action: &zz_terminal::TerminalViewAction,
) -> Option<CopyModeRefreshRequest> {
    let (zz_terminal::TerminalViewAction::CopyMode(action)
    | zz_terminal::TerminalViewAction::CopyModeCounted { action, .. }) = action
    else {
        return None;
    };
    match action {
        zz_terminal::CopyModeAction::RefreshOn => Some(CopyModeRefreshRequest::On),
        zz_terminal::CopyModeAction::RefreshOff => Some(CopyModeRefreshRequest::Off),
        zz_terminal::CopyModeAction::RefreshToggle => Some(CopyModeRefreshRequest::Toggle),
        _ => None,
    }
}

pub(super) fn terminal_view_action_refreshes_now(action: &zz_terminal::TerminalViewAction) -> bool {
    matches!(
        action,
        zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::RefreshNow)
            | zz_terminal::TerminalViewAction::CopyModeCounted {
                action: zz_terminal::CopyModeAction::RefreshNow,
                ..
            }
    )
}

pub(super) fn retarget_copy_mode_tables(inner: &mut ServerState, changed_window: Option<WindowId>) {
    let retargeted = inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.copy_session.as_ref().map(|value| (id, value)))
        .filter(|(client, session)| {
            !session.exiting
                && inner
                    .client(**client)
                    .and_then(|c| c.command_output.as_ref())
                    .is_none_or(|output| output.parked)
        })
        .filter_map(|(client, session)| {
            let window = inner.engine.state.window_for_pane(session.pane)?;
            changed_window
                .is_none_or(|changed| changed == window)
                .then_some((*client, session.pane))
        })
        .chain(
            inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.command_output.as_ref().map(|value| (id, value)))
                .filter_map(|(client, output)| {
                    if output.parked {
                        return None;
                    }
                    let window = inner.engine.state.window_for_pane(output.pane)?;
                    changed_window
                        .is_none_or(|changed| changed == window)
                        .then_some((*client, output.pane))
                }),
        )
        .collect::<Vec<_>>();
    let restored = inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.command_output.as_ref().map(|_| id))
        .filter_map(|client| {
            let session = inner
                .client(*client)
                .and_then(|c| c.copy_session.as_ref())
                .filter(|session| !session.exiting)?;
            let window = inner.engine.state.window_for_pane(session.pane)?;
            changed_window
                .is_none_or(|changed| changed == window)
                .then_some((*client, session.pane))
        })
        .collect::<Vec<_>>();
    for (client, pane) in retargeted {
        let Ok(table) = inner.engine.copy_mode_table_for_pane(pane) else {
            continue;
        };
        let table = table.to_owned();
        inner
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(Some(table));
    }
    for (client, pane) in restored {
        let Ok(table) = inner.engine.copy_mode_table_for_pane(pane) else {
            continue;
        };
        let table = table.to_owned();
        if let Some(output) = inner
            .client_mut(client)
            .and_then(|c| c.command_output.as_mut())
        {
            output.previous_key_table = Some(table);
        }
    }
}

pub(super) fn panes_for_windows(
    inner: &ServerState,
    windows: &BTreeSet<WindowId>,
) -> BTreeSet<PaneId> {
    windows
        .iter()
        .filter_map(|window| inner.engine.state.windows.get(window))
        .flat_map(|window| window.panes.keys().copied())
        .collect()
}

pub(super) fn panes_mode_viewers(inner: &ServerState, window: WindowId) -> Vec<(ClientId, bool)> {
    inner
        .clients
        .iter()
        .filter(|(_, c)| c.subscriber.is_some() && c.kind == Some(ClientKind::Interactive))
        .filter(|(id, _)| client_focused_window_for_attachment(inner, **id) == Some(window))
        .map(|(id, c)| (*id, !c.exits_on_detach))
        .collect()
}

pub(super) fn push_pane_mode(inner: &mut ServerState, pane: PaneId, mode: PaneModeRequest) -> bool {
    let replaced = !matches!(mode, PaneModeRequest::Panes(_))
        && matches!(
            inner
                .modes
                .pane_modes
                .get(&pane)
                .and_then(|modes| modes.last()),
            Some(PaneModeRequest::Panes(_))
        )
        && pop_pane_mode(inner, pane);
    let previous = top_pane_mode_name(inner, pane);
    let current = pane_mode_name(&mode);
    let pushed = push_pane_mode_entry(inner, pane, mode) || replaced;
    if pushed {
        inner.modes.pane_mode_transitions.push(PaneModeTransition {
            pane,
            entered: true,
            previous,
            current: Some(current),
        });
    }
    pushed
}

fn push_pane_mode_entry(inner: &mut ServerState, pane: PaneId, mut mode: PaneModeRequest) -> bool {
    let frames = match &mode {
        PaneModeRequest::Panes(panes)
            if !matches!(
                inner
                    .modes
                    .pane_modes
                    .get(&pane)
                    .and_then(|modes| modes.last()),
                Some(PaneModeRequest::Panes(_))
            ) =>
        {
            Some(panes_mode_live_frames(inner, panes.source_window))
        }
        _ => None,
    };
    let modes = inner.modes.pane_modes.entry(pane).or_default();
    let existing = modes
        .iter()
        .position(|entry| std::mem::discriminant(entry) == std::mem::discriminant(&mode));
    if existing.is_some() && existing == modes.len().checked_sub(1) {
        return false;
    }
    if let Some(index) = existing {
        let mut previous = modes.remove(index);
        match (&mut previous, &mode) {
            (PaneModeRequest::Switch(previous), PaneModeRequest::Switch(next)) => {
                previous.kill_source = next.kill_source;
            }
            (PaneModeRequest::Customize(previous), PaneModeRequest::Customize(next)) => {
                previous.kill_source = next.kill_source;
            }
            _ => {}
        }
        mode = previous;
    } else if pane_mode_lifetime(&mode).1
        && let Some(window) = inner.engine.state.window_for_pane(pane)
        && inner.engine.state.windows[&window].zoomed_pane.is_none()
    {
        inner.modes.pane_mode_zooms.insert(pane);
        let _ = inner.engine.state.toggle_zoom(pane);
    }
    inner.modes.pane_modes.entry(pane).or_default().push(mode);
    if let Some(frames) = frames {
        inner.engine.set_pane_status_hidden(pane, true);
        let size = inner.engine.pane_geometry(pane);
        inner.modes.panes_mode_frames.insert(
            pane,
            PanesModeFrames {
                size,
                resized: std::cell::Cell::new(false),
                frames,
            },
        );
    }
    true
}

pub(super) fn clear_pane_modes(inner: &mut ServerState, pane: PaneId) -> bool {
    let mut changed = false;
    while pop_pane_mode(inner, pane) {
        changed = true;
    }
    changed
}

pub(super) fn clock_modes_are_open(inner: &ServerState) -> bool {
    inner.modes.pane_modes.values().any(|modes| {
        modes
            .iter()
            .any(|mode| matches!(mode, PaneModeRequest::Clock))
    })
}
