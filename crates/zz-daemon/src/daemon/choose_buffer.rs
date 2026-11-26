use super::*;

impl Shared {
    pub(super) fn input_choose_buffer(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        action: ChooseBufferAction,
    ) -> Result<(), DaemonError> {
        if kind != ClientKind::Interactive {
            return Err(ServerError::InvalidCommand(
                "choose-buffer input requires an interactive client".to_owned(),
            )
            .into());
        }
        let route = self.chooser_route(client, false);
        if let (ChooseBufferAction::Key(input), Some((source_pane, _, false, pane))) =
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
        if let ChooseBufferAction::Key(input) = &action
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
            ChooseBufferAction::Key(input) => {
                match self.chooser_key_binding(client, kind, context, source_pane, &input)? {
                    ChooserKey::Pass => ChooseBufferAction::Key(input),
                    ChooserKey::Key(key) => ChooseBufferAction::Key(key),
                    ChooserKey::Claimed => return Ok(()),
                }
            }
            ChooseBufferAction::SearchAppend(text) => {
                match self.chooser_search_key(client, kind, context, source_pane, &text)? {
                    None => ChooseBufferAction::SearchAppend(text),
                    Some((key, rest)) => {
                        if let Some(key) = key {
                            self.input_choose_buffer(
                                client,
                                kind,
                                context,
                                ChooseBufferAction::Key(key),
                            )?;
                        }
                        if rest.is_empty() {
                            return Ok(());
                        }
                        return self.input_choose_buffer(
                            client,
                            kind,
                            context,
                            ChooseBufferAction::SearchAppend(rest),
                        );
                    }
                }
            }
            action => action,
        };

        if matches!(action, ChooseBufferAction::Key(_)) {
            self.ensure_prompt_history();
        }
        let mut remembered = None;
        let (outcome, deleted) = {
            let mut inner = self.inner.lock();
            let Some(mut chooser) = inner
                .client_mut(client)
                .and_then(|c| c.choose_buffer.take())
            else {
                return Ok(());
            };
            let prompt_edit = match (&action, chooser.prompt.is_some()) {
                (ChooseBufferAction::Key(input), true) => {
                    let Some(edit) = chooser_prompt_edit(input) else {
                        inner.client_entry(client).choose_buffer.replace(chooser);
                        return Ok(());
                    };
                    Some(edit)
                }
                _ => None,
            };
            let mut search_edit = None;
            let swallowed_help = chooser.help
                && prompt_edit.is_none()
                && matches!(action, ChooseBufferAction::Key(_));
            if chooser.help && prompt_edit.is_none() {
                chooser.help = false;
            }
            let action = match action {
                _ if swallowed_help => ChooseBufferAction::Close,
                ChooseBufferAction::Key(input) if prompt_edit.is_none() => {
                    let searching = chooser.search.is_some();
                    if !searching
                        && overlay_key_action(&inner.engine.keys, "choose-buffer", &input)
                            == Some("edit")
                    {
                        let source = chooser.source_pane;
                        let edit = usize::try_from(chooser.rendered.selected)
                            .ok()
                            .and_then(|index| chooser.rendered.items.get(index))
                            .and_then(|item| {
                                inner
                                    .buffers
                                    .paste_buffers
                                    .iter()
                                    .find(|buffer| buffer.name == item.name)
                            })
                            .map(|buffer| (buffer.name.clone(), Arc::clone(&buffer.data)));
                        inner.client_entry(client).choose_buffer.replace(chooser);
                        drop(inner);
                        if let Some((name, data)) = edit {
                            self.spawn_editor(
                                client,
                                source,
                                &data,
                                EditorTarget::Buffer {
                                    name,
                                    data: Arc::clone(&data),
                                },
                            );
                        }
                        return Ok(());
                    }
                    if let Some(row) =
                        chooser_row_for_key(&chooser.rendered.items, &input, searching, |item| {
                            &item.key
                        })
                    {
                        ChooseBufferAction::PasteIndex(row)
                    } else if let Some(action) =
                        choose_buffer_key_action(&inner.engine.keys, &input, searching)
                    {
                        action
                    } else if let Some(key) =
                        searching.then(|| chooser_search_edit_key(&input)).flatten()
                    {
                        search_edit = Some(key);
                        ChooseBufferAction::Key(input)
                    } else {
                        inner.client_entry(client).choose_buffer.replace(chooser);
                        return Ok(());
                    }
                }
                action => action,
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
                (ChooseBufferInputOutcome::Close, Vec::new())
            } else {
                let result = if swallowed_help {
                    ChooseBufferResult::Rebuild
                } else if let Some(key) = search_edit {
                    let (_, separators) = inner.engine.prompt_key_options(attached_session);
                    chooser.edit_search(
                        &inner.buffers.paste_buffers,
                        key,
                        prompt_history(&inner, CommandPromptType::Search),
                        &separators,
                    );
                    ChooseBufferResult::Rebuild
                } else if let Some(edit) = prompt_edit {
                    let prompt_type = chooser
                        .prompt
                        .as_ref()
                        .map_or(CommandPromptType::Search, ChooserPrompt::history_type);
                    let (vi, separators) = inner.engine.prompt_key_options(attached_session);
                    chooser.edit_prompt(
                        &edit,
                        prompt_history(&inner, prompt_type),
                        (vi, &separators),
                        &mut remembered,
                    )
                } else {
                    if matches!(action, ChooseBufferAction::SearchAccept) {
                        remembered = chooser
                            .search
                            .as_ref()
                            .filter(|search| !search.query.is_empty())
                            .map(|search| (CommandPromptType::Search, search.query.clone()));
                    }
                    match chooser.apply(action, &inner.buffers.paste_buffers) {
                        Ok(result) => result,
                        Err(error) => {
                            inner.client_entry(client).choose_buffer.replace(chooser);
                            return Err(error.into());
                        }
                    }
                };
                let mut deleted = Vec::new();
                let kill_source = chooser.kill_source.then_some(chooser.source_pane);
                let outcome = match result {
                    ChooseBufferResult::Updated => {
                        let search = chooser.rendered.search.clone();
                        let selected = chooser.rendered.selected;
                        inner.client_entry(client).choose_buffer.replace(chooser);
                        ChooseBufferInputOutcome::Delta { search, selected }
                    }
                    ChooseBufferResult::Rebuild => {
                        let facts = borrowed_format_hook_facts(&inner);
                        chooser.rebuild(
                            &inner.engine,
                            &inner.buffers.paste_buffers,
                            attached_session,
                            &facts,
                        );
                        let state = chooser.rendered.clone();
                        inner.client_entry(client).choose_buffer.replace(chooser);
                        ChooseBufferInputOutcome::Full(Some(state))
                    }
                    ChooseBufferResult::Delete(names) => {
                        inner
                            .buffers
                            .paste_buffers
                            .retain(|buffer| !names.contains(&buffer.name));
                        deleted = names;
                        chooser.selected = None;
                        let facts = borrowed_format_hook_facts(&inner);
                        chooser.rebuild(
                            &inner.engine,
                            &inner.buffers.paste_buffers,
                            attached_session,
                            &facts,
                        );
                        if chooser.rendered.items.is_empty() {
                            ChooseBufferInputOutcome::Full(None)
                        } else {
                            let state = chooser.rendered.clone();
                            inner.client_entry(client).choose_buffer.replace(chooser);
                            ChooseBufferInputOutcome::Full(Some(state))
                        }
                    }
                    ChooseBufferResult::Paste(names) => {
                        let names = names
                            .into_iter()
                            .filter(|name| {
                                inner
                                    .buffers
                                    .paste_buffers
                                    .iter()
                                    .any(|buffer| buffer.name == *name)
                            })
                            .collect::<Vec<_>>();
                        if names.is_empty() {
                            ChooseBufferInputOutcome::Close
                        } else {
                            ChooseBufferInputOutcome::Select {
                                pane: chooser.source_pane,
                                names,
                                template: chooser.template.clone(),
                            }
                        }
                    }
                    ChooseBufferResult::Close => ChooseBufferInputOutcome::Close,
                };
                if let Some(pane) = kill_source
                    && inner
                        .client(client)
                        .is_none_or(|c| c.choose_buffer.is_none())
                {
                    inner.modes.chooser_kill_panes.push(pane);
                }
                (outcome, deleted)
            }
        };
        if let Some((prompt_type, input)) = remembered {
            self.record_prompt_history(prompt_type, &input);
        }

        if !deleted.is_empty() {
            self.run_event_hooks(
                deleted
                    .into_iter()
                    .map(|name| PendingHookEvent::paste_buffer("paste-buffer-deleted", name))
                    .collect(),
            );
        }

        match outcome {
            ChooseBufferInputOutcome::Delta { search, selected } => {
                self.publish_to_client(
                    client,
                    EventPayload::ChooseBufferUpdate { search, selected },
                );
                self.publish_chooser_presentation(client);
            }
            ChooseBufferInputOutcome::Full(state) => {
                self.publish_to_client(client, EventPayload::ChooseBuffer { state });
                self.publish_chooser_presentation(client);
                self.refresh_choose_buffers_except(Some(client));
            }
            ChooseBufferInputOutcome::Select {
                pane,
                names,
                template,
            } => {
                self.publish_to_client(client, EventPayload::ChooseBuffer { state: None });
                self.reap_chooser_kill_panes(client, kind, context);
                let pane = {
                    let inner = self.inner.lock();
                    if inner.engine.state.window_for_pane(pane).is_some() {
                        pane
                    } else {
                        context.pane.unwrap_or(pane)
                    }
                };
                for name in names {
                    if let Some(template) = template.as_deref() {
                        self.execute_chooser_command(
                            client,
                            context,
                            template,
                            &name,
                            "choose-buffer",
                            None,
                        );
                    } else {
                        self.paste_buffer_to_pane(
                            pane,
                            PasteBufferPaste {
                                requested_name: Some(&name),
                                delete: false,
                                bracketed: true,
                                separator: b"\r",
                                literal: false,
                                expected_client: Some(client),
                                event_hooks_enabled: true,
                            },
                        )?;
                    }
                }
            }
            ChooseBufferInputOutcome::Close => {
                self.publish_to_client(client, EventPayload::ChooseBuffer { state: None });
            }
        }
        self.reap_chooser_kill_panes(client, kind, context);
        self.release_closed_chooser_zoom(client);
        Ok(())
    }

    pub(super) fn refresh_choose_buffers(&self) {
        self.refresh_choose_buffers_except(None);
    }

    fn refresh_choose_buffers_except(&self, excluded: Option<ClientId>) {
        let updates = {
            let mut inner = self.inner.lock();
            let clients = inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.choose_buffer.as_ref().map(|_| id))
                .copied()
                .filter(|client| Some(*client) != excluded)
                .collect::<Vec<_>>();
            let mut updates = Vec::with_capacity(clients.len());
            for client in clients {
                let shown = chooser_shown(&inner, client);
                let Some(mut chooser) = inner
                    .client_mut(client)
                    .and_then(|c| c.choose_buffer.take())
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
                    || inner.buffers.paste_buffers.is_empty()
                {
                    updates.push((client, None));
                    continue;
                }
                if !shown {
                    inner.client_entry(client).choose_buffer.replace(chooser);
                    updates.push((client, None));
                    continue;
                }
                let facts = borrowed_format_hook_facts(&inner);
                chooser.rebuild(
                    &inner.engine,
                    &inner.buffers.paste_buffers,
                    attached_session,
                    &facts,
                );
                let state = chooser.rendered.clone();
                inner.client_entry(client).choose_buffer.replace(chooser);
                updates.push((client, Some(state)));
            }
            updates
        };
        for (client, state) in updates {
            self.publish_to_client(client, EventPayload::ChooseBuffer { state });
            self.publish_chooser_presentation(client);
        }
    }
}

const MAX_CHOOSE_BUFFER_ITEMS: usize = 4_096;

const MAX_CHOOSE_BUFFER_NAME_BYTES: usize = 512;

pub(super) const MAX_CHOOSE_BUFFER_PREVIEW_BYTES: usize = 256;

const CHOOSE_BUFFER_PAGE_ROWS: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChooseBufferResult {
    Updated,
    Close,
    Paste(Vec<String>),
    Delete(Vec<String>),
    Rebuild,
}

enum ChooseBufferInputOutcome {
    Delta {
        search: Option<ChooseBufferSearchState>,
        selected: u32,
    },
    Full(Option<ChooseBufferState>),
    Select {
        pane: PaneId,
        names: Vec<String>,
        template: Option<String>,
    },
    Close,
}

#[derive(Debug)]
pub(super) struct ChooseBufferSession {
    pub(super) source_pane: PaneId,
    pub(super) source_session: SessionId,
    pub(super) filter: Option<String>,
    format: Option<String>,
    kill_source: bool,
    tagged: BTreeSet<String>,
    help: bool,
    pub(super) sort: TmuxSort,
    key_format: Option<String>,
    pub(super) template: Option<String>,
    pub(super) names: Vec<String>,
    pub(super) selected: Option<String>,
    pub(super) search: Option<ChooseBufferSearchState>,
    pub(super) search_editor: Option<ModePrompt>,
    last_search: Option<ChooseBufferSearchState>,
    pub(super) rendered: ChooseBufferState,
    pub(super) presentation_rows: Vec<ChooserRow>,
    pub(super) preview_size: ChooserPreviewSize,
    pub(super) prompt: Option<ChooserPrompt>,
}

impl ChooseBufferSession {
    pub(super) fn new(
        source_pane: PaneId,
        engine: &MuxEngine,
        buffers: &[PasteBuffer],
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
        filter: Option<String>,
        format: Option<String>,
        kill_source: bool,
        sort: TmuxSort,
        key_format: Option<String>,
    ) -> Result<Option<Self>, ServerError> {
        let source_window = engine
            .state
            .window_for_pane(source_pane)
            .ok_or_else(|| ServerError::MissingTarget(source_pane.to_string()))?;
        let source_session = engine.state.windows[&source_window].session;
        if buffers.is_empty() {
            return Ok(None);
        }
        let mut chooser = Self {
            source_pane,
            source_session,
            filter,
            format,
            kill_source,
            tagged: BTreeSet::new(),
            help: false,
            sort,
            key_format,
            template: None,
            names: Vec::new(),
            selected: None,
            search: None,
            search_editor: None,
            last_search: None,
            rendered: ChooseBufferState {
                items: Vec::new(),
                search: None,
                selected: 0,
                filter_no_matches: false,
                help: false,
                prompt: String::new(),
            },
            presentation_rows: Vec::new(),
            preview_size: ChooserPreviewSize::Normal,
            prompt: None,
        };
        chooser.rebuild(engine, buffers, attached_session, facts);
        Ok(Some(chooser))
    }

    pub(super) fn rebuild(
        &mut self,
        engine: &MuxEngine,
        buffers: &[PasteBuffer],
        attached_session: Option<SessionId>,
        facts: &dyn crate::status::FormatFactSource,
    ) {
        self.names.clear();
        let source_context = ExecutionContext::for_pane(&engine.state, self.source_pane);
        let mut sorted = buffers.iter().collect::<Vec<_>>();
        self.sort.apply(&mut sorted, |left, right| {
            let ordering = match self.sort.order() {
                Some(TmuxSortOrder::Creation) => right.created.cmp(&left.created),
                Some(TmuxSortOrder::Name) => left.name.cmp(&right.name),
                Some(TmuxSortOrder::Size) => left.data.len().cmp(&right.data.len()),
                _ => std::cmp::Ordering::Equal,
            };
            ordering.then_with(|| left.name.cmp(&right.name))
        });
        let filtered = |apply_filter: bool| {
            sorted
                .iter()
                .copied()
                .filter(|buffer| {
                    if !apply_filter {
                        return true;
                    }
                    let (Some(filter), Some(source_context)) =
                        (self.filter.as_deref(), source_context.as_ref())
                    else {
                        return true;
                    };
                    let mut hooks =
                        DaemonFormatHooks::command(facts).with_buffer(buffer_format_facts(buffer));
                    format_true(&engine.expand_pane_format(
                        filter,
                        source_context,
                        attached_session,
                        FormatClient::NoClient,
                        &mut hooks,
                    ))
                })
                .collect::<Vec<_>>()
        };
        let mut visible = filtered(self.filter.is_some());
        let filter_no_matches = visible.is_empty() && self.filter.is_some();
        if filter_no_matches {
            visible = filtered(false);
        }
        let mut items = Vec::with_capacity(visible.len().min(MAX_CHOOSE_BUFFER_ITEMS));
        for (line, buffer) in visible
            .into_iter()
            .take(MAX_CHOOSE_BUFFER_ITEMS)
            .enumerate()
        {
            self.names.push(buffer.name.clone());
            let key = match (self.key_format.as_deref(), source_context.as_ref()) {
                (None, _) => default_chooser_row_key(line),
                (Some(format), Some(source_context)) => {
                    let variables = chooser_row_variables(line);
                    let mut hooks = DaemonFormatHooks::command_with_variables(facts, &variables)
                        .with_buffer(buffer_format_facts(buffer));
                    parsed_chooser_row_key(&engine.expand_pane_format(
                        format,
                        source_context,
                        attached_session,
                        FormatClient::NoClient,
                        &mut hooks,
                    ))
                }
                (Some(_), None) => String::new(),
            };
            let text = match (self.format.as_deref(), source_context.as_ref()) {
                (None, _) | (Some(_), None) => String::new(),
                (Some(format), Some(source_context)) => {
                    let mut hooks =
                        DaemonFormatHooks::command(facts).with_buffer(buffer_format_facts(buffer));
                    bounded_choose_item_text(&engine.expand_pane_format(
                        format,
                        source_context,
                        attached_session,
                        FormatClient::NoClient,
                        &mut hooks,
                    ))
                }
            };
            items.push(ChooseBufferItem {
                name: bounded_choose_buffer_name(&buffer.name),
                preview: bounded_choose_buffer_preview(&buffer.data),
                size_bytes: u64::try_from(buffer.data.len()).unwrap_or(u64::MAX),
                created_unix_seconds: buffer
                    .created
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                key,
                text,
                tagged: self.tagged.contains(&buffer.name),
            });
        }
        let previous_index = usize::try_from(self.rendered.selected).unwrap_or(usize::MAX);
        let selected = self
            .selected
            .as_ref()
            .and_then(|name| self.names.iter().position(|candidate| candidate == name))
            .unwrap_or(previous_index)
            .min(items.len().saturating_sub(1));
        self.selected = self.names.get(selected).cloned();
        self.tagged.retain(|name| self.names.contains(name));
        self.rendered = ChooseBufferState {
            items,
            search: self.search.clone(),
            selected: u32::try_from(selected).unwrap_or(u32::MAX),
            filter_no_matches,
            help: self.help,
            prompt: self
                .prompt
                .as_ref()
                .map_or_else(String::new, ChooserPrompt::line),
        };
        self.presentation_rows = chooser_presentation::buffer_rows(
            engine,
            buffers,
            &self.names,
            &self.rendered.items,
            self.format.is_some(),
            source_context.as_ref(),
            attached_session,
            facts,
        );
    }

    pub(super) fn edit_search(
        &mut self,
        buffers: &[PasteBuffer],
        key: ModeKey,
        history: &[String],
        separators: &str,
    ) {
        let (Some(search), Some(editor)) = (self.search.as_mut(), self.search_editor.as_mut())
        else {
            return;
        };
        editor.set_key_options(false, separators);
        editor.key_with_history(key, history);
        search.query = editor.input();
        let search = search.clone();
        self.rendered.search = Some(search.clone());
        self.select_search_match(buffers, &search.query, search.reverse, true);
    }

    fn edit_prompt(
        &mut self,
        edit: &ChooserPromptEdit,
        history: &[String],
        options: (bool, &str),
        remembered: &mut Option<(CommandPromptType, String)>,
    ) -> ChooseBufferResult {
        let Some(prompt) = self.prompt.as_mut() else {
            return ChooseBufferResult::Updated;
        };
        match prompt.edit(edit, history, options) {
            PromptOutcome::Cancelled => self.prompt = None,
            PromptOutcome::Done => {
                *remembered = prompt.remembered();
                let input = prompt.input();
                self.prompt = None;
                self.filter = (!input.is_empty()).then_some(input);
            }
            _ => {}
        }
        ChooseBufferResult::Rebuild
    }

    pub(super) fn apply(
        &mut self,
        action: ChooseBufferAction,
        buffers: &[PasteBuffer],
    ) -> Result<ChooseBufferResult, ServerError> {
        let len = self.rendered.items.len();
        if len == 0 {
            return Ok(ChooseBufferResult::Close);
        }
        let current = usize::try_from(self.rendered.selected)
            .unwrap_or(usize::MAX)
            .min(len - 1);
        match action {
            ChooseBufferAction::Previous => self.select_index(current.saturating_sub(1)),
            ChooseBufferAction::Next => self.select_index((current + 1).min(len - 1)),
            ChooseBufferAction::PagePrevious => {
                self.select_index(current.saturating_sub(CHOOSE_BUFFER_PAGE_ROWS));
            }
            ChooseBufferAction::PageNext => {
                self.select_index(current.saturating_add(CHOOSE_BUFFER_PAGE_ROWS).min(len - 1));
            }
            ChooseBufferAction::First => self.select_index(0),
            ChooseBufferAction::Last => self.select_index(len - 1),
            ChooseBufferAction::Select(index) => {
                self.select_index(usize::try_from(index).unwrap_or(usize::MAX));
            }
            ChooseBufferAction::PasteIndex(index) => {
                self.select_index(usize::try_from(index).unwrap_or(usize::MAX));
                return Ok(self
                    .selected
                    .clone()
                    .map_or(ChooseBufferResult::Updated, |name| {
                        ChooseBufferResult::Paste(vec![name])
                    }));
            }
            ChooseBufferAction::Paste => {
                return Ok(self
                    .selected
                    .clone()
                    .map_or(ChooseBufferResult::Updated, |name| {
                        ChooseBufferResult::Paste(vec![name])
                    }));
            }
            ChooseBufferAction::Delete => {
                return Ok(self
                    .selected
                    .clone()
                    .map_or(ChooseBufferResult::Updated, |name| {
                        ChooseBufferResult::Delete(vec![name])
                    }));
            }
            ChooseBufferAction::SearchStart { reverse } => {
                self.search = Some(ChooseBufferSearchState {
                    query: String::new(),
                    reverse,
                });
                self.search_editor = Some(ModePrompt::new("(search) ", "", ""));
                self.rendered.search.clone_from(&self.search);
            }
            ChooseBufferAction::SearchAppend(text) => {
                let Some(search) = self.search.as_ref() else {
                    return Ok(ChooseBufferResult::Updated);
                };
                if search.query.len().saturating_add(text.len()) > MAX_CHOOSE_BUFFER_QUERY_BYTES {
                    return Err(ServerError::InvalidCommand(format!(
                        "choose-buffer search exceeds {MAX_CHOOSE_BUFFER_QUERY_BYTES} bytes"
                    )));
                }
                for character in text.chars() {
                    self.edit_search(buffers, ModeKey::Char(character), &[], "");
                }
            }
            ChooseBufferAction::SearchBackspace => {
                self.edit_search(buffers, ModeKey::Backspace, &[], "");
            }
            ChooseBufferAction::SearchAccept => {
                self.search_editor = None;
                if let Some(search) = self.search.take()
                    && !search.query.is_empty()
                {
                    self.last_search = Some(search);
                }
                self.rendered.search = None;
            }
            ChooseBufferAction::SearchCancel => {
                self.search_editor = None;
                self.search = None;
                self.rendered.search = None;
            }
            ChooseBufferAction::SearchNext { reverse } => {
                if let Some(search) = self.last_search.clone() {
                    self.select_search_match(
                        buffers,
                        &search.query,
                        search.reverse ^ reverse,
                        false,
                    );
                }
            }
            ChooseBufferAction::Close => return Ok(ChooseBufferResult::Close),
            ChooseBufferAction::Tag => {
                if let Some(name) = self.names.get(current).cloned()
                    && !self.tagged.remove(&name)
                {
                    self.tagged.insert(name);
                }
                self.select_index((current + 1).min(len - 1));
                return Ok(ChooseBufferResult::Rebuild);
            }
            ChooseBufferAction::TagNone => {
                self.tagged.clear();
                return Ok(ChooseBufferResult::Rebuild);
            }
            ChooseBufferAction::TagAll => {
                self.tagged = self.names.iter().cloned().collect();
                return Ok(ChooseBufferResult::Rebuild);
            }
            ChooseBufferAction::SortNext => {
                self.sort.next_order(&zz_mux::WINDOW_BUFFER_ORDER_SEQ);
                return Ok(ChooseBufferResult::Rebuild);
            }
            ChooseBufferAction::SortReverse => {
                self.sort.reverse();
                return Ok(ChooseBufferResult::Rebuild);
            }
            ChooseBufferAction::Help => {
                self.help = true;
                return Ok(ChooseBufferResult::Rebuild);
            }
            ChooseBufferAction::PreviewCycle => {
                self.preview_size = chooser_presentation::next_preview_size(self.preview_size);
                return Ok(ChooseBufferResult::Updated);
            }
            ChooseBufferAction::FilterPrompt => {
                self.prompt = Some(ChooserPrompt::new(
                    ChooserPromptKind::Filter,
                    "(filter) ".to_owned(),
                    self.filter.as_deref().unwrap_or_default(),
                    Vec::new(),
                ));
                return Ok(ChooseBufferResult::Rebuild);
            }
            ChooseBufferAction::ClearFilter => {
                self.prompt = None;
                self.filter = None;
                return Ok(ChooseBufferResult::Rebuild);
            }
            ChooseBufferAction::CollapseAll | ChooseBufferAction::ExpandAll => {
                return Ok(ChooseBufferResult::Rebuild);
            }
            ChooseBufferAction::DeleteTagged => {
                return Ok(ChooseBufferResult::Delete(self.tagged_names()));
            }
            ChooseBufferAction::PasteTagged => {
                return Ok(ChooseBufferResult::Paste(self.tagged_names()));
            }
            ChooseBufferAction::Key(_) => {}
        }
        Ok(ChooseBufferResult::Updated)
    }

    fn tagged_names(&self) -> Vec<String> {
        self.names
            .iter()
            .filter(|name| self.tagged.contains(*name))
            .cloned()
            .collect()
    }

    pub(super) fn select_index(&mut self, index: usize) {
        let index = index.min(self.names.len().saturating_sub(1));
        self.rendered.selected = u32::try_from(index).unwrap_or(u32::MAX);
        self.selected = self.names.get(index).cloned();
    }

    pub(super) fn select_search_match(
        &mut self,
        buffers: &[PasteBuffer],
        query: &str,
        reverse: bool,
        include_current: bool,
    ) {
        if query.is_empty() || self.names.is_empty() {
            return;
        }
        let len = self.names.len();
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
            let name = &self.names[index];
            let Some(buffer) = buffers.iter().find(|buffer| &buffer.name == name) else {
                continue;
            };
            if contains_buffer_query(buffer.name.as_bytes(), query)
                || contains_buffer_query(&buffer.data, query)
            {
                self.select_index(index);
                return;
            }
        }
    }
}

fn contains_buffer_query(haystack: &[u8], query: &str) -> bool {
    let needle = query.as_bytes();
    if !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|candidate| candidate == needle)
    {
        return true;
    }
    !needle.is_empty()
        && needle.is_ascii()
        && haystack
            .windows(needle.len())
            .any(|candidate| candidate.eq_ignore_ascii_case(needle))
}

fn bounded_choose_buffer_name(name: &str) -> String {
    if name.len() <= MAX_CHOOSE_BUFFER_NAME_BYTES {
        return name.to_owned();
    }
    let mut end = MAX_CHOOSE_BUFFER_NAME_BYTES.saturating_sub('…'.len_utf8());
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &name[..end])
}

fn bounded_choose_buffer_preview(data: &[u8]) -> String {
    let mut preview = String::with_capacity(MAX_CHOOSE_BUFFER_PREVIEW_BYTES);
    let prefix_len = data.len().min(MAX_CHOOSE_BUFFER_PREVIEW_BYTES);
    let sample = String::from_utf8_lossy(&data[..prefix_len]);
    let mut truncated = prefix_len < data.len();
    for character in sample.chars() {
        let mut encoded = [0; 4];
        let fragment = match character {
            '\n' => " ⏎ ",
            '\r' => "",
            '\t' => " ⇥ ",
            character if character.is_control() => "�",
            character => character.encode_utf8(&mut encoded),
        };
        if preview.len().saturating_add(fragment.len())
            > MAX_CHOOSE_BUFFER_PREVIEW_BYTES.saturating_sub('…'.len_utf8())
        {
            truncated = true;
            break;
        }
        preview.push_str(fragment);
    }
    if truncated {
        preview.push('…');
    }
    preview
}
