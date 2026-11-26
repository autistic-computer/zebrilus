use super::*;

impl Shared {
    pub(super) fn input_choose_tree(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        action: ChooseTreeAction,
    ) -> Result<(), DaemonError> {
        if kind != ClientKind::Interactive {
            return Err(ServerError::InvalidCommand(
                "choose-tree input requires an interactive client".to_owned(),
            )
            .into());
        }
        let route = self.chooser_route(client, true);
        if let (ChooseTreeAction::Key(input), Some((source_pane, _, false, pane))) =
            (&action, route)
        {
            return self.input_key(
                client,
                kind,
                context,
                pane.unwrap_or(source_pane),
                input.clone(),
                false,
            );
        }
        if let ChooseTreeAction::Key(input) = &action
            && input.action == zz_terminal::KeyAction::Release
        {
            let _ = self.key_decision(client, &input_key_name(input), true);
            return Ok(());
        }
        let Some((source_pane, true, _, _)) = route else {
            return Ok(());
        };
        let read_only = self.inner.lock().attachment.client_flags.contains(client);
        self.note_terminal_input_without_bell(client, source_pane);
        if read_only {
            return Ok(());
        }
        let action = match action {
            ChooseTreeAction::Key(input) => {
                match self.chooser_key_binding(client, kind, context, source_pane, &input)? {
                    ChooserKey::Pass => ChooseTreeAction::Key(input),
                    ChooserKey::Key(key) => ChooseTreeAction::Key(key),
                    ChooserKey::Claimed => return Ok(()),
                }
            }
            ChooseTreeAction::SearchAppend(text) => {
                match self.chooser_search_key(client, kind, context, source_pane, &text)? {
                    None => ChooseTreeAction::SearchAppend(text),
                    Some((key, rest)) => {
                        if let Some(key) = key {
                            self.input_choose_tree(
                                client,
                                kind,
                                context,
                                ChooseTreeAction::Key(key),
                            )?;
                        }
                        if rest.is_empty() {
                            return Ok(());
                        }
                        return self.input_choose_tree(
                            client,
                            kind,
                            context,
                            ChooseTreeAction::SearchAppend(rest),
                        );
                    }
                }
            }
            action => action,
        };
        if matches!(action, ChooseTreeAction::Key(_)) {
            self.ensure_prompt_history();
        }
        let mut remembered = None;
        let (result, state, delta, command, runs) = {
            let mut inner = self.inner.lock();
            let Some(mut chooser) = inner.client_mut(client).and_then(|c| c.choose_tree.take())
            else {
                return Ok(());
            };
            let prompt_step = match (&action, chooser.prompt.as_ref().map(|prompt| prompt.kind)) {
                (ChooseTreeAction::Key(input), Some(ChooserPromptKind::Kill)) => {
                    let Some(answer) = chooser_prompt_answer(input) else {
                        inner.client_entry(client).choose_tree.replace(chooser);
                        return Ok(());
                    };
                    Some(ChooserPromptStep::Single(answer))
                }
                (
                    ChooseTreeAction::Key(input),
                    Some(ChooserPromptKind::Command | ChooserPromptKind::Filter),
                ) => {
                    let Some(edit) = chooser_prompt_edit(input) else {
                        inner.client_entry(client).choose_tree.replace(chooser);
                        return Ok(());
                    };
                    Some(ChooserPromptStep::Edit(edit))
                }
                _ => None,
            };
            let dismissed_help = chooser.help && prompt_step.is_none();
            if dismissed_help {
                chooser.help = false;
                chooser.rendered.help = false;
            }
            let attached_session = client_attached_session(&inner, client);
            let facts = borrowed_format_hook_facts(&inner);
            if chooser.kind == ChooseTreeKind::Clients {
                chooser.clients = chooser_presentation::client_chooser_rows(
                    &inner,
                    chooser.format.as_deref(),
                    chooser.filter.as_deref(),
                );
            }
            let filter_before = chooser.filter.clone();
            let result = if let Some(step) = prompt_step {
                match step {
                    ChooserPromptStep::Single(answer) => chooser.answer_kill_prompt(answer),
                    ChooserPromptStep::Edit(edit) => {
                        let prompt_type = chooser
                            .prompt
                            .as_ref()
                            .map_or(CommandPromptType::Command, ChooserPrompt::history_type);
                        let (vi, separators) = inner.engine.prompt_key_options(attached_session);
                        chooser.edit_command_prompt(
                            &edit,
                            prompt_history(&inner, prompt_type),
                            (vi, &separators),
                            &mut remembered,
                        )
                    }
                }
            } else if dismissed_help && matches!(action, ChooseTreeAction::Key(_)) {
                ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full)
            } else if let Some(key) = match &action {
                ChooseTreeAction::Key(input) if chooser.search.is_some() => {
                    chooser_search_edit_key(input)
                }
                _ => None,
            } {
                let (_, separators) = inner.engine.prompt_key_options(attached_session);
                chooser.edit_search(
                    key,
                    prompt_history(&inner, CommandPromptType::Search),
                    &separators,
                );
                ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full)
            } else {
                let action = match action {
                    ChooseTreeAction::Key(input) => {
                        let searching = chooser.search.is_some();
                        if let Some(row) = chooser_row_for_key(
                            &chooser.rendered.items,
                            &input,
                            searching,
                            |item| &item.key,
                        ) {
                            ChooseTreeAction::ActivateIndex(row)
                        } else if let Some(action) =
                            choose_tree_key_action(&inner.engine.keys, &input, searching)
                        {
                            action
                        } else if let Some(action) = (chooser.kind == ChooseTreeKind::Clients
                            && !searching)
                            .then(|| crate::keys::client_mode_key_action(&input))
                            .flatten()
                        {
                            action
                        } else {
                            inner.client_entry(client).choose_tree.replace(chooser);
                            return Ok(());
                        }
                    }
                    action => action,
                };
                if matches!(action, ChooseTreeAction::SearchAccept) {
                    remembered = chooser
                        .search
                        .as_ref()
                        .filter(|search| !search.query.is_empty())
                        .map(|search| (CommandPromptType::Search, search.query.clone()));
                }
                match chooser.apply(action, &inner.engine, attached_session, &facts) {
                    Ok(result) => result,
                    Err(error) => {
                        inner.client_entry(client).choose_tree.replace(chooser);
                        return Err(error.into());
                    }
                }
            };
            if chooser.filter != filter_before {
                let previous = (chooser.rendered.selected, chooser.selected);
                if chooser.kind == ChooseTreeKind::Clients {
                    chooser.clients = chooser_presentation::client_chooser_rows(
                        &inner,
                        chooser.format.as_deref(),
                        chooser.filter.as_deref(),
                    );
                }
                chooser.rebuild(&inner.engine, attached_session, &facts);
                if previous.1.is_some_and(|target| {
                    !chooser
                        .rendered
                        .items
                        .iter()
                        .any(|item| item.target == target)
                }) {
                    let last = chooser.rendered.items.len().saturating_sub(1);
                    let index = usize::try_from(previous.0).unwrap_or(usize::MAX).min(last);
                    chooser.rendered.selected = u32::try_from(index).unwrap_or(0);
                    chooser.selected = chooser.rendered.items.get(index).map(|item| item.target);
                }
            }
            let state = (result == ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full))
                .then(|| chooser.rendered.clone());
            let delta = (result == ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta))
                .then(|| (chooser.rendered.selected, chooser.rendered.search.clone()));
            let command = match (&result, chooser.template.clone()) {
                (ChooseTreeResult::Activate(target), Some(template)) => Some((
                    template,
                    choose_tree_command_target(&inner.engine, &chooser.clients, *target),
                )),
                _ => None,
            };
            let runs: Vec<(String, String, Option<ExecutionContext>)> = match &result {
                ChooseTreeResult::Kill(targets) => targets
                    .iter()
                    .filter_map(|target| {
                        choose_tree_command_target(&inner.engine, &chooser.clients, *target)
                            .ok()
                            .map(|name| (choose_tree_kill_command(*target).to_owned(), name, None))
                    })
                    .collect(),
                ChooseTreeResult::Command { template, targets } => targets
                    .iter()
                    .filter_map(|target| {
                        choose_tree_command_target(&inner.engine, &chooser.clients, *target)
                            .ok()
                            .map(|name| {
                                (
                                    template.clone(),
                                    name,
                                    choose_tree_target_context(&inner.engine, *target),
                                )
                            })
                    })
                    .collect(),
                _ => Vec::new(),
            };
            if matches!(
                result,
                ChooseTreeResult::Updated(_)
                    | ChooseTreeResult::Kill(_)
                    | ChooseTreeResult::Command { .. }
                    | ChooseTreeResult::Swap { .. }
                    | ChooseTreeResult::Mark(_)
            ) {
                inner.client_entry(client).choose_tree.replace(chooser);
            } else if chooser.kill_source {
                inner.modes.chooser_kill_panes.push(chooser.source_pane);
            }
            (result, state, delta, command, runs)
        };
        if let Some((prompt_type, input)) = remembered {
            self.record_prompt_history(prompt_type, &input);
        }

        match result {
            ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full) => {
                self.publish_to_client(client, EventPayload::ChooseTree { state });
                self.publish_chooser_presentation(client);
            }
            ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta) => {
                let (selected, search) = delta.expect("updated chooser retains cursor state");
                self.publish_to_client(client, EventPayload::ChooseTreeUpdate { search, selected });
                self.publish_chooser_presentation(client);
            }
            ChooseTreeResult::Close => {
                self.publish_to_client(client, EventPayload::ChooseTree { state: None });
                self.publish_chooser_presentation(client);
            }
            ChooseTreeResult::Kill(_) | ChooseTreeResult::Command { .. } => {
                let row = self
                    .inner
                    .lock()
                    .client(client)
                    .and_then(|c| c.choose_tree.as_ref())
                    .map(|chooser| chooser.rendered.selected);
                let mut rebuild: Option<OverlaySourceThen> = Some(Box::new(move |shared| {
                    {
                        let mut inner = shared.inner.lock();
                        if let Some(chooser) = inner
                            .client_mut(client)
                            .and_then(|c| c.choose_tree.as_mut())
                        {
                            chooser.pending_row = row;
                        }
                    }
                    shared.refresh_choose_trees();
                }));
                let client_target =
                    ExecutionContext::new(context.session, context.window, context.pane);
                let last = runs.len().checked_sub(1);
                for (index, (template, name, row_target)) in runs.into_iter().enumerate() {
                    context.retarget(row_target.as_ref().unwrap_or(&client_target));
                    let then = (Some(index) == last).then(|| rebuild.take()).flatten();
                    self.execute_chooser_command(
                        client,
                        context,
                        &template,
                        &name,
                        "choose-tree",
                        then,
                    );
                }
                context.retarget(&client_target);
                if let Some(rebuild) = rebuild {
                    rebuild(self);
                }
            }
            ChooseTreeResult::Swap {
                current,
                other,
                row,
            } => {
                {
                    let mut inner = self.inner.lock();
                    inner
                        .engine
                        .state
                        .swap_windows_keeping_current(current, other)?;
                    if let Some(chooser) = inner
                        .client_mut(client)
                        .and_then(|c| c.choose_tree.as_mut())
                    {
                        chooser.pending_row = Some(row);
                    }
                }
                self.publish_snapshot();
                self.refresh_choose_trees();
            }
            ChooseTreeResult::Mark(pane) => {
                {
                    let mut inner = self.inner.lock();
                    match pane {
                        Some(pane) => inner.engine.state.set_marked_pane(pane)?,
                        None => inner.engine.state.clear_marked_pane(),
                    }
                }
                self.publish_snapshot();
                self.refresh_choose_trees();
            }
            ChooseTreeResult::Activate(target) => {
                self.publish_to_client(client, EventPayload::ChooseTree { state: None });
                self.reap_chooser_kill_panes(client, kind, context);
                let activated = if let Some((template, replacement)) = command {
                    match replacement {
                        Ok(replacement) => self.execute_chooser_command(
                            client,
                            context,
                            &template,
                            &replacement,
                            "choose-tree",
                            None,
                        ),
                        Err(error) => {
                            let error = DaemonError::from(error);
                            self.publish_background_command_error(client, context, &error, true);
                        }
                    }
                    Ok(())
                } else {
                    self.activate_choose_tree_target(client, kind, context, target)
                };
                activated?;
            }
        }
        self.reap_chooser_kill_panes(client, kind, context);
        self.release_closed_chooser_zoom(client);
        Ok(())
    }

    fn activate_choose_tree_target(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        target: ChooseTreeTarget,
    ) -> Result<(), DaemonError> {
        let (selection, session) = {
            let inner = self.inner.lock();
            let (selection, session) = match target {
                ChooseTreeTarget::Client(_) => return Ok(()),
                ChooseTreeTarget::Session(session) => (None, session),
                ChooseTreeTarget::Window(window) => {
                    let session = inner
                        .engine
                        .state
                        .windows
                        .get(&window)
                        .map(|window| window.session)
                        .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
                    (
                        Some(CommandInvocation::new(
                            "select-window",
                            ["-t", &window.to_string()],
                        )),
                        session,
                    )
                }
                ChooseTreeTarget::Pane(pane) => {
                    let window = inner
                        .engine
                        .state
                        .window_for_pane(pane)
                        .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                    let session = inner.engine.state.windows[&window].session;
                    (
                        Some(CommandInvocation::new(
                            "select-pane",
                            ["-t", &pane.to_string()],
                        )),
                        session,
                    )
                }
            };
            (selection, session)
        };
        if let Some(selection) = selection {
            self.execute_gesture(client, kind, context, "choose_tree_activate", &selection)?;
        }
        if client_attached_session(&self.inner.lock(), client) != Some(session) {
            self.execute_gesture(
                client,
                kind,
                context,
                "choose_tree_activate",
                &CommandInvocation::new("attach-session", ["-t", &session.to_string()]),
            )?;
        }
        Ok(())
    }

    pub(super) fn refresh_choose_trees(&self) {
        let updates = {
            let mut inner = self.inner.lock();
            let clients = inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.choose_tree.as_ref().map(|_| id))
                .copied()
                .collect::<Vec<_>>();
            let mut updates = Vec::with_capacity(clients.len());
            for client in clients {
                let shown = chooser_shown(&inner, client);
                let Some(mut chooser) = inner.client_mut(client).and_then(|c| c.choose_tree.take())
                else {
                    continue;
                };
                let attached_session = client_attached_session(&inner, client);
                let source_session = inner
                    .engine
                    .state
                    .window_for_pane(chooser.source_pane)
                    .map(|window| inner.engine.state.windows[&window].session);
                if attached_session != Some(chooser.source_session)
                    || source_session != Some(chooser.source_session)
                {
                    updates.push((client, None));
                    continue;
                }
                if !shown {
                    inner.client_entry(client).choose_tree.replace(chooser);
                    updates.push((client, None));
                    continue;
                }
                let facts = borrowed_format_hook_facts(&inner);
                if chooser.kind == ChooseTreeKind::Clients {
                    chooser.clients = chooser_presentation::client_chooser_rows(
                        &inner,
                        chooser.format.as_deref(),
                        chooser.filter.as_deref(),
                    );
                }
                chooser.rebuild(&inner.engine, attached_session, &facts);
                let state = chooser.rendered.clone();
                inner.client_entry(client).choose_tree.replace(chooser);
                updates.push((client, Some(state)));
            }
            updates
        };
        for (client, state) in updates {
            self.publish_to_client(client, EventPayload::ChooseTree { state });
            self.publish_chooser_presentation(client);
        }
    }
}

const MAX_CHOOSE_TREE_ITEMS: usize = 4_096;

const MAX_CHOOSE_TREE_ITEM_BYTES: usize = 512;

const CHOOSE_TREE_PAGE_ROWS: usize = 10;

fn choose_tree_kill_prompt(engine: &MuxEngine, target: ChooseTreeTarget) -> Option<String> {
    match target {
        ChooseTreeTarget::Session(session) => engine
            .state
            .sessions
            .get(&session)
            .map(|session| format!("Kill session {}? ", session.name)),
        ChooseTreeTarget::Window(window) => engine
            .state
            .windows
            .get(&window)
            .map(|window| format!("Kill window {}? ", window.index)),
        ChooseTreeTarget::Pane(pane) => {
            let window = engine.state.window_for_pane(pane)?;
            let index = engine.state.windows[&window]
                .panes
                .keys()
                .position(|candidate| *candidate == pane)?;
            Some(format!("Kill pane {index}? "))
        }
        ChooseTreeTarget::Client(_) => None,
    }
}

fn choose_tree_row_pane(engine: &MuxEngine, target: ChooseTreeTarget) -> Option<PaneId> {
    let state = &engine.state;
    let window = match target {
        ChooseTreeTarget::Pane(pane) => return state.window_for_pane(pane).map(|_| pane),
        ChooseTreeTarget::Window(window) => window,
        ChooseTreeTarget::Session(session) => state.sessions.get(&session)?.active_window,
        ChooseTreeTarget::Client(_) => return None,
    };
    Some(state.windows.get(&window)?.active_pane)
}

fn choose_tree_kill_command(target: ChooseTreeTarget) -> &'static str {
    match target {
        ChooseTreeTarget::Session(_) => "kill-session -t %%",
        ChooseTreeTarget::Window(_) => "kill-window -t %%",
        ChooseTreeTarget::Pane(_) => "kill-pane -t %%",
        ChooseTreeTarget::Client(_) => "detach-client -P -t %%",
    }
}

fn choose_tree_target_context(
    engine: &MuxEngine,
    target: ChooseTreeTarget,
) -> Option<ExecutionContext> {
    let state = &engine.state;
    let (session, window, pane) = match target {
        ChooseTreeTarget::Session(session) => {
            let window = state.sessions.get(&session)?.active_window;
            (session, window, state.windows.get(&window)?.active_pane)
        }
        ChooseTreeTarget::Window(window) => {
            let window_state = state.windows.get(&window)?;
            (window_state.session, window, window_state.active_pane)
        }
        ChooseTreeTarget::Pane(pane) => {
            let window = state.window_for_pane(pane)?;
            (state.windows.get(&window)?.session, window, pane)
        }
        ChooseTreeTarget::Client(_) => return None,
    };
    Some(ExecutionContext::new(
        Some(session),
        Some(window),
        Some(pane),
    ))
}

pub(super) fn choose_tree_command_target(
    engine: &MuxEngine,
    clients: &[ClientChooserRow],
    target: ChooseTreeTarget,
) -> Result<String, ServerError> {
    match target {
        ChooseTreeTarget::Client(id) => clients
            .iter()
            .find(|row| row.client == id)
            .map(|row| row.name.clone())
            .ok_or_else(|| ServerError::MissingTarget(id.to_string())),
        ChooseTreeTarget::Session(session) => {
            let session = engine
                .state
                .sessions
                .get(&session)
                .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
            Ok(format!("={}:", session.name))
        }
        ChooseTreeTarget::Window(window) => {
            let window_state = engine
                .state
                .windows
                .get(&window)
                .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
            let session = engine
                .state
                .sessions
                .get(&window_state.session)
                .ok_or_else(|| ServerError::MissingTarget(window_state.session.to_string()))?;
            Ok(format!("={}:{}.", session.name, window_state.index))
        }
        ChooseTreeTarget::Pane(pane) => {
            let window = engine
                .state
                .window_for_pane(pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            let window_state = engine
                .state
                .windows
                .get(&window)
                .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
            let session = engine
                .state
                .sessions
                .get(&window_state.session)
                .ok_or_else(|| ServerError::MissingTarget(window_state.session.to_string()))?;
            Ok(format!("={}:{}.{}", session.name, window_state.index, pane))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChooseTreeResult {
    Updated(ChooseTreeUpdateKind),
    Close,
    Activate(ChooseTreeTarget),
    Kill(Vec<ChooseTreeTarget>),
    Command {
        template: String,
        targets: Vec<ChooseTreeTarget>,
    },
    Swap {
        current: zz_protocol::WindowId,
        other: zz_protocol::WindowId,
        row: u32,
    },
    Mark(Option<PaneId>),
}

enum ChooserPromptStep {
    Single(char),
    Edit(ChooserPromptEdit),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChooseTreeUpdateKind {
    Delta,
    Full,
}

const CLIENT_MODE_DETACH_COMMAND: &str = "detach-client -t %%";

#[derive(Debug)]
pub(super) struct ChooseTreeSession {
    pub(super) source_pane: PaneId,
    pub(super) source_session: SessionId,
    pub(super) kind: ChooseTreeKind,
    pub(super) info_preview: bool,
    pub(super) clients: Vec<ClientChooserRow>,
    pub(super) sessions_only: bool,
    pub(super) filter: Option<String>,
    format: Option<String>,
    pub(super) hide_source: bool,
    kill_source: bool,
    pub(super) prompt_accept: bool,
    tagged: BTreeSet<ChooseTreeTarget>,
    pub(super) prompt: Option<ChooserPrompt>,
    help: bool,
    pending_row: Option<u32>,
    pub(super) sort: TmuxSort,
    key_format: Option<String>,
    pub(super) template: Option<String>,
    expanded_sessions: BTreeSet<SessionId>,
    expanded_windows: BTreeSet<zz_protocol::WindowId>,
    built: Option<(BTreeSet<SessionId>, BTreeSet<zz_protocol::WindowId>)>,
    pub(super) selected: Option<ChooseTreeTarget>,
    pub(super) search: Option<ChooseTreeSearchState>,
    pub(super) search_editor: Option<ModePrompt>,
    last_search: Option<ChooseTreeSearchState>,
    pub(super) rendered: ChooseTreeState,
    pub(super) presentation_rows: Vec<ChooserRow>,
    pub(super) preview_size: ChooserPreviewSize,
}

impl ChooseTreeSession {
    #[allow(clippy::fn_params_excessive_bools)]
    pub(super) fn new(
        kind: ChooseTreeKind,
        info_preview: bool,
        clients: Vec<ClientChooserRow>,
        sessions_only: bool,
        source_pane: PaneId,
        engine: &MuxEngine,
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
        filter: Option<String>,
        format: Option<String>,
        hide_source: bool,
        kill_source: bool,
        prompt_accept: bool,
        sort: TmuxSort,
        key_format: Option<String>,
    ) -> Result<Self, ServerError> {
        let source_window = engine
            .state
            .window_for_pane(source_pane)
            .ok_or_else(|| ServerError::MissingTarget(source_pane.to_string()))?;
        let source_session = engine.state.windows[&source_window].session;
        let expanded_sessions = if sessions_only {
            BTreeSet::new()
        } else {
            engine.state.sessions.keys().copied().collect()
        };
        let expanded_windows = if kind == ChooseTreeKind::Panes {
            engine.state.windows.keys().copied().collect()
        } else {
            BTreeSet::new()
        };
        let selected = if kind == ChooseTreeKind::Clients {
            None
        } else {
            Some(if sessions_only {
                ChooseTreeTarget::Session(source_session)
            } else {
                match kind {
                    ChooseTreeKind::Windows => ChooseTreeTarget::Window(source_window),
                    ChooseTreeKind::Panes
                        if engine.state.windows[&source_window].panes.len() == 1 =>
                    {
                        ChooseTreeTarget::Window(source_window)
                    }
                    ChooseTreeKind::Panes | ChooseTreeKind::Clients => {
                        ChooseTreeTarget::Pane(source_pane)
                    }
                }
            })
        };
        let mut chooser = Self {
            source_pane,
            source_session,
            kind,
            info_preview,
            clients,
            sessions_only,
            filter,
            format,
            hide_source,
            kill_source,
            prompt_accept,
            tagged: BTreeSet::new(),
            prompt: None,
            help: false,
            pending_row: None,
            sort,
            key_format,
            template: None,
            expanded_sessions,
            expanded_windows,
            built: None,
            selected,
            search: None,
            search_editor: None,
            last_search: None,
            rendered: ChooseTreeState {
                items: Vec::new(),
                search: None,
                selected: 0,
                kind,
                filter_no_matches: false,
                prompt: String::new(),
                help: false,
            },
            presentation_rows: Vec::new(),
            preview_size: ChooserPreviewSize::Normal,
        };
        chooser.rebuild(engine, attached_session, facts);
        Ok(chooser)
    }

    pub(super) fn branches(
        &self,
        engine: &MuxEngine,
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
        apply_filter: bool,
    ) -> Vec<(SessionId, Vec<(WindowId, Vec<PaneId>)>)> {
        let state = &engine.state;
        let mut sessions = state
            .sessions_by_name()
            .into_iter()
            .map(|session| session.id)
            .collect::<Vec<_>>();
        self.sort.apply(&mut sessions, |left, right| {
            let left = &state.sessions[left];
            let right = &state.sessions[right];
            let ordering = match self.sort.order() {
                Some(TmuxSortOrder::Activity) => right.sort_activity.cmp(&left.sort_activity),
                Some(TmuxSortOrder::Creation) => left.sort_created.cmp(&right.sort_created),
                Some(TmuxSortOrder::Index) => left.id.cmp(&right.id),
                Some(TmuxSortOrder::Name) => left.name.cmp(&right.name),
                _ => std::cmp::Ordering::Equal,
            };
            ordering.then_with(|| left.name.cmp(&right.name))
        });

        let mut branches = Vec::new();
        for session_id in sessions {
            let Some(session) = state.sessions.get(&session_id) else {
                continue;
            };
            let mut windows = session
                .windows
                .iter()
                .copied()
                .filter(|window| state.windows.contains_key(window))
                .collect::<Vec<_>>();
            self.sort.apply(&mut windows, |left, right| {
                self.window_sort_ordering(engine, *left, *right)
            });

            let mut visible_windows = Vec::new();
            for window_id in windows {
                let window = &state.windows[&window_id];
                let mut panes = window.pane_order().to_vec();
                self.sort.apply(&mut panes, |left, right| {
                    let left_pane = &window.panes[left];
                    let right_pane = &window.panes[right];
                    let left_index = window
                        .pane_order()
                        .iter()
                        .position(|pane| pane == left)
                        .unwrap_or_default();
                    let right_index = window
                        .pane_order()
                        .iter()
                        .position(|pane| pane == right)
                        .unwrap_or_default();
                    let ordering = match self.sort.order() {
                        Some(TmuxSortOrder::Activity) => {
                            left_pane.active_point.cmp(&right_pane.active_point)
                        }
                        Some(TmuxSortOrder::Creation) => left.cmp(right),
                        Some(TmuxSortOrder::Index) => left_index.cmp(&right_index),
                        Some(TmuxSortOrder::Name) => left_pane.title.cmp(&right_pane.title),
                        Some(TmuxSortOrder::Size) => {
                            let left_extent =
                                window.displayed_pane_geometry(*left).unwrap_or_default();
                            let right_extent =
                                window.displayed_pane_geometry(*right).unwrap_or_default();
                            (u32::from(left_extent.0) * u32::from(left_extent.1))
                                .cmp(&(u32::from(right_extent.0) * u32::from(right_extent.1)))
                        }
                        Some(TmuxSortOrder::Z) => window.pane_z(*left).cmp(&window.pane_z(*right)),
                        _ => std::cmp::Ordering::Equal,
                    };
                    ordering.then_with(|| left_pane.title.cmp(&right_pane.title))
                });
                if apply_filter {
                    panes.retain(|pane| {
                        let (Some(filter), Some(target)) = (
                            self.filter.as_deref(),
                            ExecutionContext::for_pane(state, *pane),
                        ) else {
                            return true;
                        };
                        let mut hooks = DaemonFormatHooks::command(facts);
                        format_true(&engine.expand_pane_format(
                            filter,
                            &target,
                            attached_session,
                            FormatClient::NoClient,
                            &mut hooks,
                        ))
                    });
                }
                let window_visible = !apply_filter || !panes.is_empty();
                if self.hide_source {
                    panes.retain(|pane| *pane != self.source_pane);
                }
                if window_visible {
                    visible_windows.push((window_id, panes));
                }
            }
            if !apply_filter || !visible_windows.is_empty() {
                branches.push((session_id, visible_windows));
            }
        }
        branches
    }

    fn window_sort_ordering(
        &self,
        engine: &MuxEngine,
        left: WindowId,
        right: WindowId,
    ) -> std::cmp::Ordering {
        let state = &engine.state;
        let (Some(left), Some(right)) = (state.windows.get(&left), state.windows.get(&right))
        else {
            return std::cmp::Ordering::Equal;
        };
        let area = |window: WindowId| {
            let columns = engine
                .window_extent(window, zz_protocol::Axis::Horizontal)
                .unwrap_or_default();
            let rows = engine
                .window_extent(window, zz_protocol::Axis::Vertical)
                .unwrap_or_default();
            u32::from(columns) * u32::from(rows)
        };
        let ordering = match self.sort.order() {
            Some(TmuxSortOrder::Activity) => right.activity.cmp(&left.activity),
            Some(TmuxSortOrder::Creation) => left.created.cmp(&right.created),
            Some(TmuxSortOrder::Index) => left.index.cmp(&right.index),
            Some(TmuxSortOrder::Name) => left.name.cmp(&right.name),
            Some(TmuxSortOrder::Size) => area(left.id).cmp(&area(right.id)),
            _ => std::cmp::Ordering::Equal,
        };
        ordering.then_with(|| left.name.cmp(&right.name))
    }

    fn assign_row_keys(
        &self,
        items: &mut [ChooseTreeItem],
        engine: &MuxEngine,
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
    ) {
        let Some(format) = self.key_format.as_deref() else {
            for (line, item) in items.iter_mut().enumerate() {
                item.key = default_chooser_row_key(line);
            }
            return;
        };
        for (line, item) in items.iter_mut().enumerate() {
            let context = match item.target {
                ChooseTreeTarget::Session(session) => {
                    Some(ExecutionContext::new(Some(session), None, None))
                }
                ChooseTreeTarget::Window(window) => engine
                    .state
                    .windows
                    .get(&window)
                    .map(|entry| ExecutionContext::new(Some(entry.session), Some(window), None)),
                ChooseTreeTarget::Pane(pane) => ExecutionContext::for_pane(&engine.state, pane),
                ChooseTreeTarget::Client(_) => Some(ExecutionContext::new(None, None, None)),
            };
            item.key = context.map_or_else(String::new, |context| {
                let variables = chooser_row_variables(line);
                let mut hooks = DaemonFormatHooks::command_with_variables(facts, &variables);
                parsed_chooser_row_key(&engine.expand_target_format(
                    format,
                    &context,
                    attached_session,
                    FormatClient::NoClient,
                    &mut hooks,
                ))
            });
        }
    }

    fn assign_row_text(
        &self,
        items: &mut [ChooseTreeItem],
        engine: &MuxEngine,
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
    ) {
        let Some(format) = self.format.as_deref() else {
            return;
        };
        for item in items.iter_mut() {
            let context = match item.target {
                ChooseTreeTarget::Session(session) => {
                    Some(ExecutionContext::new(Some(session), None, None))
                }
                ChooseTreeTarget::Window(window) => engine
                    .state
                    .windows
                    .get(&window)
                    .map(|entry| ExecutionContext::new(Some(entry.session), Some(window), None)),
                ChooseTreeTarget::Pane(pane) => ExecutionContext::for_pane(&engine.state, pane),
                ChooseTreeTarget::Client(_) => Some(ExecutionContext::new(None, None, None)),
            };
            item.text = context.map_or_else(String::new, |context| {
                let mut hooks = DaemonFormatHooks::command(facts);
                bounded_choose_item_text(&engine.expand_target_format(
                    format,
                    &context,
                    attached_session,
                    FormatClient::NoClient,
                    &mut hooks,
                ))
            });
        }
    }

    fn expand_rows_the_last_build_dropped(
        &mut self,
        branches: &[(SessionId, Vec<(zz_protocol::WindowId, Vec<PaneId>)>)],
    ) {
        let sessions = branches
            .iter()
            .map(|(session, _)| *session)
            .collect::<BTreeSet<_>>();
        let windows = branches
            .iter()
            .flat_map(|(_, windows)| windows.iter().map(|(window, _)| *window))
            .collect::<BTreeSet<_>>();
        if let Some((built_sessions, built_windows)) = self.built.take() {
            for session in &sessions {
                if !built_sessions.contains(session) {
                    self.expanded_sessions.insert(*session);
                }
            }
            for window in &windows {
                if !built_windows.contains(window) {
                    self.expanded_windows.insert(*window);
                }
            }
        }
        self.built = Some((sessions, windows));
    }

    pub(super) fn rebuild(
        &mut self,
        engine: &MuxEngine,
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
    ) {
        if self.kind == ChooseTreeKind::Clients {
            self.rebuild_clients(engine, attached_session, facts);
            return;
        }
        let state = &engine.state;
        let mut branches = self.branches(engine, attached_session, facts, self.filter.is_some());
        let filter_no_matches = branches.is_empty() && self.filter.is_some();
        if filter_no_matches {
            branches = self.branches(engine, attached_session, facts, false);
        }
        self.expand_rows_the_last_build_dropped(&branches);
        let mut items = Vec::new();
        'sessions: for (session_id, windows) in branches {
            let session = &state.sessions[&session_id];
            let session_has_children = !windows.is_empty();
            let session_expanded =
                session_has_children && self.expanded_sessions.contains(&session.id);
            if !push_choose_tree_item(
                &mut items,
                ChooseTreeItem {
                    label: bounded_choose_tree_text(&session.name),
                    detail: format_count(session.windows.len(), "window"),
                    target: ChooseTreeTarget::Session(session.id),
                    depth: 0,
                    flags: choose_tree_flags(
                        session_expanded,
                        session_has_children,
                        attached_session == Some(session.id),
                    ),
                    pane_kind: None,
                    key: String::new(),
                    text: String::new(),
                },
            ) {
                break;
            }
            if !session_expanded {
                continue;
            }

            for (window_id, panes) in windows {
                let window = &state.windows[&window_id];
                let window_has_children = !panes.is_empty();
                let window_expanded =
                    window_has_children && self.expanded_windows.contains(&window_id);
                if !push_choose_tree_item(
                    &mut items,
                    ChooseTreeItem {
                        label: bounded_choose_tree_text(&format!(
                            "{}:{}",
                            window.index, window.name
                        )),
                        detail: format_count(window.panes.len(), "pane"),
                        target: ChooseTreeTarget::Window(window_id),
                        depth: 1,
                        flags: choose_tree_flags(
                            window_expanded,
                            window_has_children,
                            session.active_window == window_id,
                        ),
                        pane_kind: None,
                        key: String::new(),
                        text: String::new(),
                    },
                ) {
                    break 'sessions;
                }
                if !window_expanded {
                    continue;
                }

                for pane_id in panes {
                    let Some(pane) = window.panes.get(&pane_id) else {
                        continue;
                    };
                    let (pane_kind, detail) = match &pane.kind {
                        PaneKind::Picker { .. } => (None, "Choose pane type".to_owned()),
                        PaneKind::Terminal => {
                            (Some(ChooseTreePaneKind::Terminal), "Terminal".to_owned())
                        }
                        PaneKind::Browser(browser) => (
                            Some(ChooseTreePaneKind::Browser),
                            format!("Browser · {} · {}", browser.profile, browser.url()),
                        ),
                        PaneKind::Editor(editor) => (
                            Some(ChooseTreePaneKind::Editor),
                            editor.path.as_ref().map_or_else(
                                || "Editor · scratch".to_owned(),
                                |path| format!("Editor · {path}"),
                            ),
                        ),
                        PaneKind::Agent(agent) => (
                            Some(ChooseTreePaneKind::Agent),
                            format!("Agent · {}", agent.provider.label()),
                        ),
                        PaneKind::View(view) => {
                            (None, format!("View · {}", view.content.kind_name()))
                        }
                        PaneKind::Simulator(_) => (None, "Simulator".to_owned()),
                    };
                    if !push_choose_tree_item(
                        &mut items,
                        ChooseTreeItem {
                            label: bounded_choose_tree_text(&pane.title),
                            detail: bounded_choose_tree_text(&detail),
                            target: ChooseTreeTarget::Pane(pane_id),
                            depth: 2,
                            flags: choose_tree_flags(false, false, window.active_pane == pane_id),
                            pane_kind,
                            key: String::new(),
                            text: String::new(),
                        },
                    ) {
                        break 'sessions;
                    }
                }
            }
        }

        self.assign_row_keys(&mut items, engine, attached_session, facts);
        self.assign_row_text(&mut items, engine, attached_session, facts);

        let fallback = state.window_for_pane(self.source_pane).map(|window| {
            if self.sessions_only {
                ChooseTreeTarget::Session(self.source_session)
            } else {
                match self.kind {
                    ChooseTreeKind::Windows => ChooseTreeTarget::Window(window),
                    ChooseTreeKind::Panes | ChooseTreeKind::Clients
                        if state.windows[&window].panes.len() == 1 =>
                    {
                        ChooseTreeTarget::Window(window)
                    }
                    ChooseTreeKind::Panes | ChooseTreeKind::Clients => {
                        ChooseTreeTarget::Pane(self.source_pane)
                    }
                }
            }
        });
        self.finish_rebuild(
            items,
            filter_no_matches,
            fallback,
            engine,
            attached_session,
            facts,
        );
    }

    fn rebuild_clients(
        &mut self,
        engine: &MuxEngine,
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
    ) {
        let mut rows = self.clients.clone();
        let filter_no_matches = self.filter.is_some() && !rows.iter().any(|row| row.matches);
        if !filter_no_matches {
            rows.retain(|row| row.matches);
        }
        let sort = self.sort;
        sort.apply(&mut rows, |left, right| {
            let ordering = match sort.order() {
                Some(TmuxSortOrder::Activity) => right.activity.cmp(&left.activity),
                Some(TmuxSortOrder::Creation) => left.created.cmp(&right.created),
                Some(TmuxSortOrder::Size) => {
                    (left.width, left.height).cmp(&(right.width, right.height))
                }
                _ => left.name.cmp(&right.name),
            };
            ordering.then_with(|| left.name.cmp(&right.name))
        });
        let mut items = Vec::new();
        for row in &rows {
            if !push_choose_tree_item(
                &mut items,
                ChooseTreeItem {
                    label: bounded_choose_tree_text(&row.name),
                    detail: String::new(),
                    target: ChooseTreeTarget::Client(row.client),
                    depth: 0,
                    flags: 0,
                    pane_kind: None,
                    key: String::new(),
                    text: bounded_choose_item_text(&row.text),
                },
            ) {
                break;
            }
        }
        self.assign_row_keys(&mut items, engine, attached_session, facts);
        self.finish_rebuild(
            items,
            filter_no_matches,
            None,
            engine,
            attached_session,
            facts,
        );
    }

    fn finish_rebuild(
        &mut self,
        items: Vec<ChooseTreeItem>,
        filter_no_matches: bool,
        fallback: Option<ChooseTreeTarget>,
        engine: &MuxEngine,
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
    ) {
        let selected = self
            .pending_row
            .take()
            .map(|row| usize::try_from(row).unwrap_or(usize::MAX))
            .or_else(|| {
                self.selected
                    .and_then(|target| items.iter().position(|item| item.target == target))
            })
            .or_else(|| {
                fallback.and_then(|target| items.iter().position(|item| item.target == target))
            })
            .unwrap_or(0)
            .min(items.len().saturating_sub(1));
        self.selected = items.get(selected).map(|item| item.target);
        self.rendered = ChooseTreeState {
            items,
            search: self.search.clone(),
            selected: u32::try_from(selected).unwrap_or(u32::MAX),
            kind: self.kind,
            filter_no_matches,
            prompt: self
                .prompt
                .as_ref()
                .map_or_else(String::new, ChooserPrompt::line),
            help: self.help,
        };
        self.tagged.retain(|target| {
            self.rendered
                .items
                .iter()
                .any(|item| item.target == *target)
        });
        self.mark_tagged_rows();
        self.presentation_rows = chooser_presentation::tree_rows(
            engine,
            &self.rendered.items,
            self.format.is_some(),
            attached_session,
            facts,
        );
    }

    fn mark_tagged_rows(&mut self) {
        for item in &mut self.rendered.items {
            if self.tagged.contains(&item.target) {
                item.flags |= ChooseTreeItem::TAGGED;
            } else {
                item.flags &= !ChooseTreeItem::TAGGED;
            }
        }
    }

    pub(super) fn apply(
        &mut self,
        action: ChooseTreeAction,
        engine: &MuxEngine,
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
    ) -> Result<ChooseTreeResult, ServerError> {
        let len = self.rendered.items.len();
        let current = usize::try_from(self.rendered.selected)
            .unwrap_or(usize::MAX)
            .min(len.saturating_sub(1));
        let mut update = ChooseTreeUpdateKind::Delta;
        match action {
            ChooseTreeAction::Previous => self.select_index(current.saturating_sub(1)),
            ChooseTreeAction::Next => self.select_index((current + 1).min(len.saturating_sub(1))),
            ChooseTreeAction::PagePrevious => {
                self.select_index(current.saturating_sub(CHOOSE_TREE_PAGE_ROWS));
            }
            ChooseTreeAction::PageNext => self.select_index(
                current
                    .saturating_add(CHOOSE_TREE_PAGE_ROWS)
                    .min(len.saturating_sub(1)),
            ),
            ChooseTreeAction::First => self.select_index(0),
            ChooseTreeAction::Last => self.select_index(len.saturating_sub(1)),
            ChooseTreeAction::Select(index) => {
                self.select_index(usize::try_from(index).unwrap_or(usize::MAX));
            }
            ChooseTreeAction::ActivateIndex(index) => {
                self.select_index(usize::try_from(index).unwrap_or(usize::MAX));
                return Ok(self.selected.map_or(
                    ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta),
                    ChooseTreeResult::Activate,
                ));
            }
            ChooseTreeAction::Collapse => {
                if let Some(item) = self.rendered.items.get(current).cloned() {
                    if item.expanded() {
                        self.set_expanded(item.target, false);
                        self.rebuild(engine, attached_session, facts);
                        update = ChooseTreeUpdateKind::Full;
                    } else if item.depth > 0
                        && let Some(parent) = self.rendered.items[..current]
                            .iter()
                            .rev()
                            .position(|candidate| candidate.depth < item.depth)
                    {
                        self.select_index(current - parent - 1);
                    }
                }
            }
            ChooseTreeAction::Expand => {
                if let Some(item) = self.rendered.items.get(current).cloned() {
                    if item.has_children() && !item.expanded() {
                        self.set_expanded(item.target, true);
                        self.rebuild(engine, attached_session, facts);
                        update = ChooseTreeUpdateKind::Full;
                    } else {
                        self.select_index((current + 1).min(len.saturating_sub(1)));
                    }
                }
            }
            ChooseTreeAction::Activate => {
                return Ok(self.selected.map_or(
                    ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta),
                    ChooseTreeResult::Activate,
                ));
            }
            ChooseTreeAction::SearchStart { reverse } => {
                self.search = Some(ChooseTreeSearchState {
                    query: String::new(),
                    reverse,
                });
                self.search_editor = Some(ModePrompt::new("(search) ", "", ""));
                self.rendered.search.clone_from(&self.search);
            }
            ChooseTreeAction::SearchAppend(text) => {
                let Some(search) = self.search.as_ref() else {
                    return Ok(ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta));
                };
                if search.query.len().saturating_add(text.len()) > MAX_CHOOSE_TREE_QUERY_BYTES {
                    return Err(ServerError::InvalidCommand(format!(
                        "choose-tree search exceeds {MAX_CHOOSE_TREE_QUERY_BYTES} bytes"
                    )));
                }
                for character in text.chars() {
                    self.edit_search(ModeKey::Char(character), &[], "");
                }
            }
            ChooseTreeAction::SearchBackspace => {
                self.edit_search(ModeKey::Backspace, &[], "");
            }
            ChooseTreeAction::SearchAccept => {
                self.search_editor = None;
                if let Some(search) = self.search.take()
                    && !search.query.is_empty()
                {
                    self.last_search = Some(search);
                }
                self.rendered.search = None;
            }
            ChooseTreeAction::SearchCancel => {
                self.search_editor = None;
                self.search = None;
                self.rendered.search = None;
            }
            ChooseTreeAction::SearchNext { reverse } => {
                if let Some(search) = self.last_search.clone() {
                    self.select_search_match(&search.query, search.reverse ^ reverse, false);
                }
            }
            ChooseTreeAction::Close => return Ok(ChooseTreeResult::Close),
            ChooseTreeAction::Tag => {
                if let Some(target) = self.rendered.items.get(current).map(|item| item.target) {
                    if self.tagged.remove(&target) {
                        self.mark_tagged_rows();
                    } else {
                        for relative in self.relatives(current) {
                            self.tagged.remove(&relative);
                        }
                        self.tagged.insert(target);
                        self.mark_tagged_rows();
                    }
                    self.select_index((current + 1).min(len.saturating_sub(1)));
                    update = ChooseTreeUpdateKind::Full;
                }
            }
            ChooseTreeAction::TagNone => {
                self.tagged.clear();
                self.mark_tagged_rows();
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::TagAll => {
                self.tagged = self
                    .rendered
                    .items
                    .iter()
                    .filter(|item| item.depth == 0)
                    .map(|item| item.target)
                    .collect();
                self.mark_tagged_rows();
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::KillCurrent => {
                let Some(target) = self.rendered.items.get(current).map(|item| item.target) else {
                    return Ok(ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta));
                };
                if matches!(target, ChooseTreeTarget::Client(_)) {
                    return Ok(ChooseTreeResult::Kill(vec![target]));
                }
                let Some(text) = choose_tree_kill_prompt(engine, target) else {
                    return Ok(ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta));
                };
                return Ok(self.raise_kill_prompt(text, vec![target]));
            }
            ChooseTreeAction::KillTagged => {
                let targets = self
                    .rendered
                    .items
                    .iter()
                    .filter(|item| self.tagged.contains(&item.target))
                    .map(|item| item.target)
                    .collect::<Vec<_>>();
                if targets.is_empty() {
                    return Ok(ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta));
                }
                if self.kind == ChooseTreeKind::Clients {
                    return Ok(ChooseTreeResult::Kill(targets));
                }
                let text = format!("Kill {} tagged? ", targets.len());
                return Ok(self.raise_kill_prompt(text, targets));
            }
            ChooseTreeAction::ClientDetach => {
                let Some(target) = self.rendered.items.get(current).map(|item| item.target) else {
                    return Ok(ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta));
                };
                if !matches!(target, ChooseTreeTarget::Client(_)) {
                    return Ok(ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta));
                }
                return Ok(ChooseTreeResult::Command {
                    template: CLIENT_MODE_DETACH_COMMAND.to_owned(),
                    targets: vec![target],
                });
            }
            ChooseTreeAction::ClientDetachTagged => {
                let targets = self
                    .rendered
                    .items
                    .iter()
                    .filter(|item| self.tagged.contains(&item.target))
                    .filter(|item| matches!(item.target, ChooseTreeTarget::Client(_)))
                    .map(|item| item.target)
                    .collect::<Vec<_>>();
                if targets.is_empty() {
                    return Ok(ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta));
                }
                return Ok(ChooseTreeResult::Command {
                    template: CLIENT_MODE_DETACH_COMMAND.to_owned(),
                    targets,
                });
            }
            ChooseTreeAction::ClientInfo => {
                self.info_preview = !self.info_preview;
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::SwapUp => return Ok(self.swap(engine, current, false)),
            ChooseTreeAction::SwapDown => return Ok(self.swap(engine, current, true)),
            ChooseTreeAction::SortNext => {
                if self.kind == ChooseTreeKind::Clients {
                    self.sort.next_order(&zz_mux::WINDOW_CLIENT_ORDER_SEQ);
                } else {
                    self.sort.next_order(&zz_mux::WINDOW_TREE_ORDER_SEQ);
                }
                self.rebuild(engine, attached_session, facts);
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::SortReverse => {
                self.sort.reverse();
                self.rebuild(engine, attached_session, facts);
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::Help => {
                self.help = true;
                self.rendered.help = true;
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::PreviewCycle => {
                self.preview_size = chooser_presentation::next_preview_size(self.preview_size);
            }
            ChooseTreeAction::FilterPrompt => {
                self.prompt = Some(ChooserPrompt::new(
                    ChooserPromptKind::Filter,
                    "(filter) ".to_owned(),
                    self.filter.as_deref().unwrap_or_default(),
                    Vec::new(),
                ));
                self.show_prompt();
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::ClearFilter => {
                self.prompt = None;
                self.rendered.prompt.clear();
                self.filter = None;
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::CollapseAll | ChooseTreeAction::ExpandAll => {
                let expanded = matches!(action, ChooseTreeAction::ExpandAll);
                let roots = self
                    .rendered
                    .items
                    .iter()
                    .filter(|item| item.depth == 0)
                    .map(|item| item.target)
                    .collect::<Vec<_>>();
                for root in roots {
                    self.set_expanded(root, expanded);
                }
                self.rebuild(engine, attached_session, facts);
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::Mark => {
                let Some(target) = self.rendered.items.get(current).map(|item| item.target) else {
                    return Ok(ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta));
                };
                return Ok(ChooseTreeResult::Mark(choose_tree_row_pane(engine, target)));
            }
            ChooseTreeAction::MarkClear => return Ok(ChooseTreeResult::Mark(None)),
            ChooseTreeAction::CommandPrompt => {
                let tagged = self
                    .rendered
                    .items
                    .iter()
                    .filter(|item| self.tagged.contains(&item.target))
                    .map(|item| item.target)
                    .collect::<Vec<_>>();
                let text = if tagged.is_empty() {
                    "(current) ".to_owned()
                } else {
                    format!("({} tagged) ", tagged.len())
                };
                let targets = if tagged.is_empty() {
                    self.rendered
                        .items
                        .get(current)
                        .map(|item| item.target)
                        .into_iter()
                        .collect()
                } else {
                    tagged
                };
                self.prompt = Some(ChooserPrompt::new(
                    ChooserPromptKind::Command,
                    text,
                    "",
                    targets,
                ));
                self.show_prompt();
                update = ChooseTreeUpdateKind::Full;
            }
            ChooseTreeAction::Key(_) => {}
        }
        Ok(ChooseTreeResult::Updated(update))
    }

    pub(super) fn swap(
        &mut self,
        engine: &MuxEngine,
        current: usize,
        down: bool,
    ) -> ChooseTreeResult {
        let stay = ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta);
        let Some(row) = self.rendered.items.get(current) else {
            return stay;
        };
        let depth = row.depth;
        let mut candidate = current;
        let other = loop {
            candidate = if down {
                match candidate.checked_add(1) {
                    Some(next) if next < self.rendered.items.len() => next,
                    _ => return stay,
                }
            } else {
                match candidate.checked_sub(1) {
                    Some(previous) => previous,
                    None => return stay,
                }
            };
            let entry = &self.rendered.items[candidate];
            if entry.depth < depth {
                return stay;
            }
            if entry.depth == depth {
                break entry;
            }
        };
        let (ChooseTreeTarget::Window(current_window), ChooseTreeTarget::Window(other_window)) =
            (row.target, other.target)
        else {
            return stay;
        };
        let state = &engine.state;
        let (Some(left), Some(right)) = (
            state.windows.get(&current_window),
            state.windows.get(&other_window),
        ) else {
            return stay;
        };
        if left.session != right.session {
            return stay;
        }
        if self.sort.order() != Some(TmuxSortOrder::Index)
            && self.window_sort_ordering(engine, current_window, other_window)
                != std::cmp::Ordering::Equal
        {
            return stay;
        }
        ChooseTreeResult::Swap {
            current: current_window,
            other: other_window,
            row: u32::try_from(candidate).unwrap_or(u32::MAX),
        }
    }

    fn relatives(&self, current: usize) -> Vec<ChooseTreeTarget> {
        let Some(row) = self.rendered.items.get(current) else {
            return Vec::new();
        };
        let depth = row.depth;
        let mut relatives = Vec::new();
        let mut wanted = depth;
        for item in self.rendered.items[..current].iter().rev() {
            if item.depth < wanted {
                wanted = item.depth;
                relatives.push(item.target);
                if wanted == 0 {
                    break;
                }
            }
        }
        for item in &self.rendered.items[current.saturating_add(1)..] {
            if item.depth <= depth {
                break;
            }
            relatives.push(item.target);
        }
        relatives
    }

    fn raise_kill_prompt(
        &mut self,
        text: String,
        targets: Vec<ChooseTreeTarget>,
    ) -> ChooseTreeResult {
        if self.prompt_accept {
            return ChooseTreeResult::Kill(targets);
        }
        self.rendered.prompt.clone_from(&text);
        self.prompt = Some(ChooserPrompt::new(
            ChooserPromptKind::Kill,
            text,
            "",
            targets,
        ));
        ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full)
    }

    fn show_prompt(&mut self) {
        self.rendered.prompt = self
            .prompt
            .as_ref()
            .map_or_else(String::new, ChooserPrompt::line);
    }

    pub(super) fn edit_search(&mut self, key: ModeKey, history: &[String], separators: &str) {
        let (Some(search), Some(editor)) = (self.search.as_mut(), self.search_editor.as_mut())
        else {
            return;
        };
        editor.set_key_options(false, separators);
        editor.key_with_history(key, history);
        search.query = editor.input();
        let search = search.clone();
        self.rendered.search = Some(search.clone());
        self.select_search_match(&search.query, search.reverse, true);
    }

    fn edit_command_prompt(
        &mut self,
        edit: &ChooserPromptEdit,
        history: &[String],
        options: (bool, &str),
        remembered: &mut Option<(CommandPromptType, String)>,
    ) -> ChooseTreeResult {
        let Some(prompt) = self.prompt.as_mut() else {
            return ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta);
        };
        match prompt.edit(edit, history, options) {
            PromptOutcome::Cancelled => {
                self.prompt = None;
                self.rendered.prompt.clear();
                return ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full);
            }
            PromptOutcome::Done => {
                let prompt = self.prompt.take().expect("the prompt was just borrowed");
                *remembered = prompt.remembered();
                self.rendered.prompt.clear();
                let input = prompt.input();
                if matches!(prompt.kind, ChooserPromptKind::Filter) {
                    self.filter = (!input.is_empty()).then_some(input);
                    return ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full);
                }
                if input.is_empty() {
                    return ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full);
                }
                return ChooseTreeResult::Command {
                    template: input,
                    targets: prompt.targets,
                };
            }
            _ => {}
        }
        self.show_prompt();
        ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full)
    }

    fn answer_kill_prompt(&mut self, answer: char) -> ChooseTreeResult {
        let Some(prompt) = self.prompt.take() else {
            return ChooseTreeResult::Updated(ChooseTreeUpdateKind::Delta);
        };
        self.rendered.prompt.clear();
        if answer.eq_ignore_ascii_case(&'y') {
            ChooseTreeResult::Kill(prompt.targets)
        } else {
            ChooseTreeResult::Updated(ChooseTreeUpdateKind::Full)
        }
    }

    pub(super) fn select_index(&mut self, index: usize) {
        let index = index.min(self.rendered.items.len().saturating_sub(1));
        self.rendered.selected = u32::try_from(index).unwrap_or(u32::MAX);
        self.selected = self.rendered.items.get(index).map(|item| item.target);
    }

    fn set_expanded(&mut self, target: ChooseTreeTarget, expanded: bool) {
        match target {
            ChooseTreeTarget::Session(session) => {
                if expanded {
                    self.expanded_sessions.insert(session);
                } else {
                    self.expanded_sessions.remove(&session);
                }
            }
            ChooseTreeTarget::Window(window) => {
                if expanded {
                    self.expanded_windows.insert(window);
                } else {
                    self.expanded_windows.remove(&window);
                }
            }
            ChooseTreeTarget::Pane(_) | ChooseTreeTarget::Client(_) => {}
        }
    }

    pub(super) fn select_search_match(
        &mut self,
        query: &str,
        reverse: bool,
        include_current: bool,
    ) {
        if query.is_empty() || self.rendered.items.is_empty() {
            return;
        }
        let query = query.to_lowercase();
        let len = self.rendered.items.len();
        let current = usize::try_from(self.rendered.selected)
            .unwrap_or(usize::MAX)
            .min(len - 1);
        let start = usize::from(!include_current);
        for offset in start..start.saturating_add(len) {
            let index = if reverse {
                (current + len - (offset % len)) % len
            } else {
                (current + offset) % len
            };
            let item = &self.rendered.items[index];
            if item.label.to_lowercase().contains(&query)
                || item.detail.to_lowercase().contains(&query)
                || item.target.to_string().to_lowercase().contains(&query)
            {
                self.select_index(index);
                return;
            }
        }
    }
}

fn push_choose_tree_item(items: &mut Vec<ChooseTreeItem>, item: ChooseTreeItem) -> bool {
    if items.len() >= MAX_CHOOSE_TREE_ITEMS {
        return false;
    }
    items.push(item);
    true
}

fn bounded_choose_tree_text(text: &str) -> String {
    if text.len() <= MAX_CHOOSE_TREE_ITEM_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_CHOOSE_TREE_ITEM_BYTES.saturating_sub('…'.len_utf8());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn format_count(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

const fn choose_tree_flags(expanded: bool, has_children: bool, active: bool) -> u8 {
    let mut flags = 0;
    if expanded {
        flags |= ChooseTreeItem::EXPANDED;
    }
    if has_children {
        flags |= ChooseTreeItem::HAS_CHILDREN;
    }
    if active {
        flags |= ChooseTreeItem::ACTIVE;
    }
    flags
}
