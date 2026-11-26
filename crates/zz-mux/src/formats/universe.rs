use super::*;

thread_local! {
    static EAGER_UNIVERSE_OVERRIDE: Cell<bool> = const { Cell::new(false) };
}

fn eager_universe() -> bool {
    EAGER_UNIVERSE_OVERRIDE.with(Cell::get)
}

#[doc(hidden)]
pub fn with_eager_universe<R>(body: impl FnOnce() -> R) -> R {
    let previous = EAGER_UNIVERSE_OVERRIDE.with(|flag| flag.replace(true));
    let result = body();
    EAGER_UNIVERSE_OVERRIDE.with(|flag| flag.set(previous));
    result
}

impl Default for FormatUniverse {
    fn default() -> Self {
        Self::with_built(FormatClient::NoClient, FormatNeeds::ALL)
    }
}

impl FormatUniverse {
    pub(super) fn with_built(format_client: FormatClient, built: FormatNeeds) -> Self {
        Self {
            format_client,
            built: AtomicU8::new(built.0),
            sessions: OnceLock::new(),
            windows: Mutex::default(),
            panes: Mutex::default(),
            options: Mutex::default(),
            environments: Mutex::default(),
            window_user_options: Mutex::default(),
        }
    }

    pub(super) fn built(&self) -> FormatNeeds {
        FormatNeeds(self.built.load(AtomicOrdering::Acquire))
    }

    pub(super) fn mark_built(&self, needs: FormatNeeds) {
        self.built.fetch_or(needs.0, AtomicOrdering::AcqRel);
    }

    fn detached(&self, engine: &MuxEngine, specs: Option<&[&'static FormatVariableSpec]>) -> Self {
        let detached = Self::with_built(self.format_client, self.built());
        let detach_items = |items: &LoopItems| -> LoopItems {
            items
                .iter()
                .map(|item| {
                    let context = if tree::borrowed_formats() {
                        let mut context = item
                            .context
                            .borrowed_child(Some(engine))
                            .detach_values(specs);
                        context.values = item.context.values.clone();
                        context
                    } else {
                        item.context.detach_values(specs)
                    };
                    FormatLoopItem {
                        context,
                        active: item.active,
                        metadata: item.metadata.clone(),
                    }
                })
                .collect::<Vec<_>>()
                .into()
        };
        if let Some(items) = self.sessions.get() {
            let _ = detached.sessions.set(detach_items(items));
        }
        *detached.windows.lock() = self
            .windows
            .lock()
            .iter()
            .map(|(key, items)| (*key, items.as_ref().map(detach_items)))
            .collect();
        *detached.panes.lock() = self
            .panes
            .lock()
            .iter()
            .map(|(key, items)| (*key, items.as_ref().map(detach_items)))
            .collect();
        *detached.options.lock() = self.options.lock().clone();
        *detached.environments.lock() = self.environments.lock().clone();
        *detached.window_user_options.lock() = self.window_user_options.lock().clone();
        detached
    }

    pub(super) fn same_owned(&self, other: &Self) -> bool {
        let same_items = |left: Option<&LoopItems>, right: Option<&LoopItems>| match (left, right) {
            (None, None) => true,
            (Some(left), Some(right)) => {
                Arc::ptr_eq(left, right)
                    || left.len() == right.len()
                        && left.iter().zip(right.iter()).all(|(left, right)| {
                            left.active == right.active
                                && left.metadata == right.metadata
                                && left.context.same_detached(&right.context)
                        })
            }
            _ => false,
        };
        if self.format_client != other.format_client
            || self.built() != other.built()
            || !same_items(self.sessions.get(), other.sessions.get())
        {
            return false;
        }
        let left_windows = self.windows.lock().clone();
        let right_windows = other.windows.lock().clone();
        if left_windows.len() != right_windows.len()
            || !left_windows.iter().all(|(key, left)| {
                right_windows
                    .get(key)
                    .is_some_and(|right| same_items(left.as_ref(), right.as_ref()))
            })
        {
            return false;
        }
        let left_panes = self.panes.lock().clone();
        let right_panes = other.panes.lock().clone();
        if left_panes.len() != right_panes.len()
            || !left_panes.iter().all(|(key, left)| {
                right_panes
                    .get(key)
                    .is_some_and(|right| same_items(left.as_ref(), right.as_ref()))
            })
        {
            return false;
        }
        let left_options = self.options.lock().clone();
        let right_options = other.options.lock().clone();
        let left_environment = self.environments.lock().clone();
        let right_environment = other.environments.lock().clone();
        let left_user_options = self.window_user_options.lock().clone();
        let right_user_options = other.window_user_options.lock().clone();
        left_options == right_options
            && left_environment == right_environment
            && left_user_options == right_user_options
    }

    pub(super) fn retained_bytes(&self) -> usize {
        let item_bytes = |items: &LoopItems| {
            items.iter().fold(0usize, |bytes, item| {
                bytes
                    .saturating_add(item.context.retained_bytes())
                    .saturating_add(std::mem::size_of::<LoopMetadata>())
                    .saturating_add(item.metadata.session_name.capacity())
                    .saturating_add(item.metadata.window_name.capacity())
            })
        };
        let mut bytes = std::mem::size_of::<Self>();
        if let Some(items) = self.sessions.get() {
            bytes = bytes.saturating_add(item_bytes(items));
        }
        for items in self.windows.lock().values().flatten() {
            bytes = bytes.saturating_add(item_bytes(items));
        }
        for items in self.panes.lock().values().flatten() {
            bytes = bytes.saturating_add(item_bytes(items));
        }
        for rows in self.options.lock().values().flatten() {
            for row in rows.iter() {
                bytes = bytes
                    .saturating_add(std::mem::size_of::<FormatOptionRow>())
                    .saturating_add(row.name.capacity())
                    .saturating_add(row.value.capacity())
                    .saturating_add(row.array_key.capacity())
                    .saturating_add(row.array_index.capacity());
            }
        }
        for rows in self.environments.lock().values().flatten() {
            for row in rows.iter() {
                bytes = bytes
                    .saturating_add(std::mem::size_of::<FormatEnvironRow>())
                    .saturating_add(row.name.capacity())
                    .saturating_add(cloned_raw_text_bytes(&row.value));
            }
        }
        for rows in self.window_user_options.lock().values().flatten() {
            for (name, value) in rows.iter() {
                bytes = bytes
                    .saturating_add(std::mem::size_of::<(String, String)>())
                    .saturating_add(name.capacity())
                    .saturating_add(value.capacity());
            }
        }
        bytes
    }

    #[cfg(test)]
    pub(crate) fn with_global_environment(rows: Vec<FormatEnvironRow>) -> Self {
        let universe = Self::default();
        universe.environments.lock().insert(None, Some(rows.into()));
        universe
    }
}

static NO_UNIVERSE: LazyLock<Arc<FormatUniverse>> = LazyLock::new(Arc::default);

impl Default for FormatUniverseRef<'_> {
    fn default() -> Self {
        Self {
            parts: Arc::clone(&NO_UNIVERSE),
            engine: None,
        }
    }
}

impl std::fmt::Debug for FormatUniverseRef<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FormatUniverseRef")
            .field("deferred", &self.engine.is_some())
            .field("built", &self.parts.built())
            .finish_non_exhaustive()
    }
}

impl<'e> FormatUniverseRef<'e> {
    pub(super) fn deferred(engine: &'e MuxEngine, format_client: FormatClient) -> Self {
        let universe = Self {
            parts: Arc::new(FormatUniverse::with_built(format_client, FormatNeeds::NONE)),
            engine: Some(engine),
        };
        if eager_universe() {
            universe.fill(FormatNeeds::ALL, "", "", "");
        }
        universe
    }

    pub(super) fn detach(
        &self,
        needs: FormatNeeds,
        session: &str,
        window: &str,
        pane: &str,
        specs: Option<&[&'static FormatVariableSpec]>,
    ) -> FormatUniverseRef<'static> {
        if self.engine.is_some() {
            let needs = if eager_universe() {
                FormatNeeds::ALL
            } else {
                needs
            };
            self.fill(needs, session, window, pane);
        }
        FormatUniverseRef {
            parts: self
                .engine
                .filter(|_| tree::borrowed_formats())
                .map_or_else(
                    || Arc::clone(&self.parts),
                    |engine| Arc::new(self.parts.detached(engine, specs)),
                ),
            engine: None,
        }
    }

    fn missed(&self, kind: FormatNeeds) {
        debug_assert!(
            self.parts.built() == FormatNeeds::ALL,
            "a detached format universe missed a {kind:?} part that its fill did not reach"
        );
    }

    pub(super) fn sessions(&self) -> LoopItems {
        if let Some(items) = self.parts.sessions.get() {
            return Arc::clone(items);
        }
        let Some(engine) = self.engine else {
            self.missed(FormatNeeds::SESSIONS);
            return LoopItems::default();
        };
        Arc::clone(
            self.parts
                .sessions
                .get_or_init(|| engine.format_session_items(self.parts.format_client).into()),
        )
    }

    pub(super) fn windows(&self, session: &str) -> Option<LoopItems> {
        let session = parse_session(session)?;
        if let Some(items) = self.parts.windows.lock().get(&session) {
            return items.clone();
        }
        let Some(engine) = self.engine else {
            self.missed(FormatNeeds::WINDOWS);
            return None;
        };
        let items = engine
            .format_window_items(session, self.parts.format_client)
            .map(LoopItems::from);
        self.parts
            .windows
            .lock()
            .entry(session)
            .or_insert(items)
            .clone()
    }

    pub(super) fn panes(&self, window: &str) -> Option<LoopItems> {
        let window = parse_window(window)?;
        if let Some(items) = self.parts.panes.lock().get(&window) {
            return items.clone();
        }
        let Some(engine) = self.engine else {
            self.missed(FormatNeeds::PANES);
            return None;
        };
        let items = engine
            .format_pane_items(window, self.parts.format_client)
            .map(LoopItems::from);
        self.parts
            .panes
            .lock()
            .entry(window)
            .or_insert(items)
            .clone()
    }

    pub(super) fn option_rows(&self, key: OptionRowsKey) -> Option<Arc<[FormatOptionRow]>> {
        if let Some(rows) = self.parts.options.lock().get(&key) {
            return rows.clone();
        }
        let Some(engine) = self.engine else {
            self.missed(FormatNeeds::OPTIONS);
            return None;
        };
        let rows = engine.format_option_rows_for(key).map(Arc::from);
        self.parts.options.lock().entry(key).or_insert(rows).clone()
    }

    fn environment(&self, session: Option<SessionId>) -> Option<Arc<[FormatEnvironRow]>> {
        if let Some(rows) = self.parts.environments.lock().get(&session) {
            return rows.clone();
        }
        let Some(engine) = self.engine else {
            self.missed(FormatNeeds::ENVIRONMENT);
            return None;
        };
        let rows: Option<Arc<[FormatEnvironRow]>> = match session {
            None => Some(engine.format_global_environment_rows().into()),
            Some(session) => engine
                .state
                .sessions
                .contains_key(&session)
                .then(|| engine.format_session_environment_rows(session).into()),
        };
        self.parts
            .environments
            .lock()
            .entry(session)
            .or_insert(rows)
            .clone()
    }

    pub(super) fn global_environment(&self) -> Arc<[FormatEnvironRow]> {
        self.environment(None).unwrap_or_default()
    }

    pub(super) fn session_environment(&self, session: &str) -> Option<Arc<[FormatEnvironRow]>> {
        self.environment(Some(parse_session(session)?))
    }

    pub(super) fn window_user_options(&self, window: &str) -> Option<Arc<[(String, String)]>> {
        let window = parse_window(window)?;
        if let Some(options) = self.parts.window_user_options.lock().get(&window) {
            return options.clone();
        }
        let Some(engine) = self.engine else {
            self.missed(FormatNeeds::WINDOWS);
            return None;
        };
        let options = engine
            .format_window_listed(window)
            .then(|| engine.format_window_user_options(window).into());
        self.parts
            .window_user_options
            .lock()
            .entry(window)
            .or_insert(options)
            .clone()
    }

    fn fill(&self, needs: FormatNeeds, session: &str, window: &str, pane: &str) {
        let all = needs.contains(FormatNeeds::ALL);
        let needs = if all { FormatNeeds::ALL } else { needs };
        let mut sessions = BTreeSet::new();
        let mut windows = BTreeSet::new();
        let mut panes = BTreeSet::new();
        sessions.extend(parse_session(session));
        windows.extend(parse_window(window));
        panes.extend(parse_pane(pane));
        if needs.contains(FormatNeeds::SESSIONS) {
            for item in self.sessions().iter() {
                sessions.extend(parse_session(&item.context.session_id));
                windows.extend(parse_window(&item.context.window_id));
                panes.extend(parse_pane(&item.context.pane_id));
            }
        }
        if needs.contains(FormatNeeds::WINDOWS) {
            for session in &sessions {
                let items = self.windows(&session.to_string()).unwrap_or_default();
                for item in items.iter() {
                    self.window_user_options(&item.context.window_id);
                    windows.extend(parse_window(&item.context.window_id));
                    panes.extend(parse_pane(&item.context.pane_id));
                }
            }
        }
        if needs.contains(FormatNeeds::PANES) {
            for window in &windows {
                let items = self.panes(&window.to_string()).unwrap_or_default();
                panes.extend(
                    items
                        .iter()
                        .filter_map(|item| parse_pane(&item.context.pane_id)),
                );
            }
        }
        if needs.contains(FormatNeeds::OPTIONS) {
            for key in [
                OptionRowsKey::Server,
                OptionRowsKey::GlobalSession,
                OptionRowsKey::GlobalWindow,
            ]
            .into_iter()
            .chain(sessions.iter().copied().map(OptionRowsKey::Session))
            .chain(windows.iter().copied().map(OptionRowsKey::Window))
            .chain(panes.iter().copied().map(OptionRowsKey::Pane))
            {
                self.option_rows(key);
            }
        }
        if needs.contains(FormatNeeds::ENVIRONMENT) {
            self.environment(None);
            for session in &sessions {
                self.environment(Some(*session));
            }
        }
        self.parts.mark_built(needs);
    }
}
