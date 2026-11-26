use super::*;

impl MuxEngine {
    pub fn seed_global_environment<I, K, V>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<RawText>,
        V: Into<RawText>,
    {
        self.global_environment = entries
            .into_iter()
            .map(|(name, value)| {
                (
                    name.into(),
                    EnvironmentEntry {
                        value: Some(value.into()),
                        hidden: false,
                    },
                )
            })
            .collect();
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    #[must_use]
    pub fn global_environment_variable(&self, name: &str) -> Option<String> {
        self.global_environment_bytes(name).map(String::from)
    }

    #[must_use]
    pub fn global_environment_bytes(&self, name: &str) -> Option<RawText> {
        self.global_environment
            .get(name)
            .and_then(|entry| entry.value.clone())
    }

    pub fn set_config_environment(
        &mut self,
        name: String,
        value: impl Into<RawText>,
        hidden: bool,
    ) {
        self.global_environment.insert(
            name.into(),
            EnvironmentEntry {
                value: Some(value.into()),
                hidden,
            },
        );
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    pub fn environment_for_session(
        &self,
        session: SessionId,
    ) -> Result<Vec<(RawText, Option<RawText>)>, ServerError> {
        if !self.state.sessions.contains_key(&session) {
            return Err(ServerError::MissingTarget(session.to_string()));
        }
        Ok(self.job_environment(Some(session)))
    }

    pub fn update_session_environment_from_client(
        &mut self,
        session: SessionId,
        client_environment: &BTreeMap<RawText, RawText>,
    ) -> Result<(), ServerError> {
        if !self.state.sessions.contains_key(&session) {
            return Err(ServerError::MissingTarget(session.to_string()));
        }
        let patterns = self.update_environment_names_for_session(Some(session));
        let retained = self.session_environments.entry(session).or_default();
        apply_client_environment_update(&mut retained.inner.lock(), &patterns, client_environment);
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
        Ok(())
    }

    pub fn retain_session_job_environment(
        &mut self,
        session: SessionId,
    ) -> Option<RetainedJobEnvironment> {
        self.state.sessions.contains_key(&session).then(|| {
            self.session_environments
                .entry(session)
                .or_default()
                .clone()
        })
    }

    pub fn job_environment_with_retained_session(
        &self,
        retained: Option<&RetainedJobEnvironment>,
    ) -> Vec<(RawText, Option<RawText>)> {
        let mut environment = BTreeMap::new();
        extend_job_environment(&mut environment, &self.global_environment);
        if let Some(retained) = retained {
            extend_job_environment(&mut environment, &retained.inner.lock());
        }
        environment.into_iter().collect()
    }

    pub fn job_environment(&self, session: Option<SessionId>) -> Vec<(RawText, Option<RawText>)> {
        let retained = session.and_then(|session| self.session_environments.get(&session));
        self.job_environment_with_retained_session(retained)
    }

    pub(super) fn set_environment(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("set-environment", args)?;
        if !(1..=2).contains(&positional.len()) {
            return Err(ServerError::CommandParse(
                "set-environment needs a variable and optional value".to_owned(),
            ));
        }
        let name = &positional[0];
        validate_environment_name(name)?;
        let target_session = if options.has("-g") {
            self.resolve_session(options.value("-t"), context.session)
                .ok()
        } else {
            Some(self.resolve_session(options.value("-t"), context.session)?)
        };
        let mut value = positional.get(1).cloned();
        if options.has("-F")
            && let Some(raw) = value.as_deref()
        {
            value = Some(expand_format_with_hooks(
                raw,
                self,
                self.environment_format_context(
                    target_session,
                    context.session,
                    context.target_format_client(),
                ),
                hooks,
            ));
        }
        if (options.has("-r") || options.has("-u")) && value.is_some() {
            let flag = if options.has("-u") { "-u" } else { "-r" };
            return Err(ServerError::InvalidCommand(format!(
                "can't specify a value with {flag}"
            )));
        }
        let value = if options.has("-u") || options.has("-r") {
            None
        } else {
            Some(
                value
                    .ok_or_else(|| ServerError::InvalidCommand("no value specified".to_owned()))?,
            )
        };
        let apply = |environment: &mut Environment| {
            if options.has("-u") {
                environment.remove(name);
            } else {
                let hidden = if options.has("-r") {
                    environment.get(name).is_some_and(|entry| entry.hidden)
                } else {
                    options.has("-h")
                };
                environment.insert(
                    name.clone(),
                    EnvironmentEntry {
                        value: value.clone(),
                        hidden,
                    },
                );
            }
        };
        if options.has("-g") {
            apply(&mut self.global_environment);
        } else {
            let retained = self
                .session_environments
                .entry(target_session.expect("local environment has a session"))
                .or_default();
            apply(&mut retained.inner.lock());
        }
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
        Ok(Execution::default())
    }

    pub(super) fn show_environment(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("show-environment", args)?;
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "show-environment accepts at most one variable".to_owned(),
            ));
        }
        if options.has("-g") {
            if let Some(target) = options.value("-t") {
                self.resolve_session(Some(target), context.session)?;
            }
            return show_environment_entries(&self.global_environment, &positional, &options);
        }
        let session = self.resolve_session(options.value("-t"), context.session)?;
        let Some(retained) = self.session_environments.get(&session) else {
            return show_environment_entries(&EMPTY_ENVIRONMENT, &positional, &options);
        };
        show_environment_entries(&retained.inner.lock(), &positional, &options)
    }

    pub(crate) fn format_global_environment_rows(&self) -> Vec<FormatEnvironRow> {
        format_environment_rows(&self.global_environment)
    }

    pub(crate) fn format_session_environment_rows(
        &self,
        session: SessionId,
    ) -> Vec<FormatEnvironRow> {
        self.session_environments
            .get(&session)
            .map(|retained| format_environment_rows(&retained.inner.lock()))
            .unwrap_or_default()
    }

    pub(super) fn update_environment_names(&self) -> impl Iterator<Item = &str> {
        self.stored_arrays
            .global_session
            .get("update-environment")
            .into_iter()
            .flat_map(|array| array.values().map(String::as_str))
    }

    fn update_environment_names_for_session(&self, session: Option<SessionId>) -> Vec<String> {
        let target = session.map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session);
        self.array_option_readback(target, "update-environment", true)
            .into_iter()
            .flat_map(|(array, _)| array.values().cloned())
            .collect()
    }

    pub(super) fn seed_session_environment(
        &mut self,
        session: SessionId,
        include_update_environment: bool,
        client_environment: Option<&BTreeMap<RawText, RawText>>,
        overlays: Vec<(String, String)>,
    ) {
        let mut environment = Environment::new();
        if include_update_environment && let Some(client_environment) = client_environment {
            let patterns = self.update_environment_names_for_session(None);
            apply_client_environment_update(&mut environment, &patterns, client_environment);
        }
        for (name, value) in overlays {
            environment.insert(
                name.into(),
                EnvironmentEntry {
                    value: Some(value.into()),
                    hidden: false,
                },
            );
        }
        *self
            .session_environments
            .entry(session)
            .or_default()
            .inner
            .lock() = environment;
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    fn environment_format_context(
        &self,
        session: Option<SessionId>,
        active_session: Option<SessionId>,
        format_client: FormatClient,
    ) -> FormatContext {
        let window = session.and_then(|session| {
            self.state
                .sessions
                .get(&session)
                .map(|session| session.active_window)
        });
        let pane = window.and_then(|window| {
            self.state
                .windows
                .get(&window)
                .map(|window| window.active_pane)
        });
        FormatContext {
            session,
            window,
            pane,
            active_session,
            format_client,
            format_type: FormatType::Pane,
        }
    }
}

fn format_environment_rows(environment: &Environment) -> Vec<FormatEnvironRow> {
    environment
        .iter()
        .map(|(name, entry)| FormatEnvironRow {
            name: name.to_string(),
            value: entry.value.clone().unwrap_or_default(),
            hidden: entry.hidden,
            removed: entry.value.is_none(),
        })
        .collect()
}

fn validate_environment_name(name: &RawText) -> Result<(), ServerError> {
    let name = name.as_bytes();
    if name.is_empty() {
        return Err(ServerError::InvalidCommand(
            "empty variable name".to_owned(),
        ));
    }
    if name.contains(&b'=') {
        return Err(ServerError::InvalidCommand(
            "variable name contains =".to_owned(),
        ));
    }
    Ok(())
}

fn extend_job_environment(target: &mut BTreeMap<RawText, Option<RawText>>, source: &Environment) {
    for (name, entry) in source {
        let value = if entry.hidden {
            None
        } else {
            entry.value.clone()
        };
        target.insert(name.clone(), value);
    }
}

fn environment_line(
    name: &RawText,
    entry: &EnvironmentEntry,
    options: &Options,
) -> Option<RawText> {
    if options.has("-h") != entry.hidden {
        return None;
    }
    let name = name.as_bytes();
    let mut line = Vec::new();
    if !options.has("-s") {
        if let Some(value) = &entry.value {
            line.extend_from_slice(name);
            line.push(b'=');
            line.extend_from_slice(value.as_bytes());
        } else {
            line.push(b'-');
            line.extend_from_slice(name);
        }
        return Some(RawText::from_bytes(line));
    }
    if let Some(value) = &entry.value {
        line.extend_from_slice(name);
        line.extend_from_slice(b"=\"");
        line.extend_from_slice(&shell_environment_escape(value.as_bytes()));
        line.extend_from_slice(b"\"; export ");
        line.extend_from_slice(name);
        line.push(b';');
    } else {
        line.extend_from_slice(b"unset ");
        line.extend_from_slice(name);
        line.push(b';');
    }
    Some(RawText::from_bytes(line))
}

fn show_environment_entries(
    environment: &Environment,
    positional: &[RawText],
    options: &Options,
) -> Result<Execution, ServerError> {
    if let Some(name) = positional.first() {
        let entry = environment
            .get(name)
            .ok_or_else(|| ServerError::InvalidCommand(format!("unknown variable: {name}")))?;
        return Ok(Execution::output(
            environment_line(name, entry, options).unwrap_or_default(),
        ));
    }
    let mut output = Vec::new();
    for line in environment
        .iter()
        .filter_map(|(name, entry)| environment_line(name, entry, options))
    {
        if !output.is_empty() {
            output.push(b'\n');
        }
        output.extend_from_slice(line.as_bytes());
    }
    Ok(Execution::output(RawText::from_bytes(output)))
}

fn shell_environment_escape(value: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(value.len());
    for byte in value {
        if matches!(byte, b'$' | b'`' | b'"' | b'\\') {
            escaped.push(b'\\');
        }
        escaped.push(*byte);
    }
    escaped
}

pub(super) fn spawn_environment(options: &Options) -> Vec<(String, String)> {
    options
        .values("-e")
        .filter_map(|value| value.split_once('='))
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

pub(super) fn session_creation_environment(options: &Options) -> Vec<(String, String)> {
    options
        .values("-e")
        .filter_map(|value| value.split_once('='))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

enum EnvironmentPattern {
    Name(String),
    Glob(GlobPattern),
}

type CompiledPatterns = Arc<[(String, EnvironmentPattern)]>;

fn compiled_update_environment(patterns: &[String]) -> CompiledPatterns {
    static COMPILED: std::sync::LazyLock<
        parking_lot::Mutex<Option<(Vec<String>, CompiledPatterns)>>,
    > = std::sync::LazyLock::new(parking_lot::Mutex::default);
    let mut cached = COMPILED.lock();
    if let Some((key, compiled)) = cached.as_ref()
        && key.as_slice() == patterns
    {
        return Arc::clone(compiled);
    }
    let compiled: CompiledPatterns = patterns
        .iter()
        .map(|pattern| {
            let glob = GlobPattern::new(pattern);
            let compiled = glob
                .literal()
                .map_or(EnvironmentPattern::Glob(glob), EnvironmentPattern::Name);
            (pattern.clone(), compiled)
        })
        .collect();
    *cached = Some((patterns.to_vec(), Arc::clone(&compiled)));
    compiled
}

pub(super) fn apply_client_environment_update(
    environment: &mut Environment,
    patterns: &[String],
    client_environment: &BTreeMap<RawText, RawText>,
) {
    for (pattern, compiled) in compiled_update_environment(patterns).iter() {
        let matches = match compiled {
            EnvironmentPattern::Name(name) => client_environment
                .range::<str, _>((
                    std::ops::Bound::Included(name.as_str()),
                    std::ops::Bound::Included(name.as_str()),
                ))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>(),
            EnvironmentPattern::Glob(glob) => client_environment
                .iter()
                .filter(|(name, _)| glob.matches(name))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>(),
        };
        if matches.is_empty() {
            environment
                .entry(pattern.clone().into())
                .and_modify(|entry| entry.value = None)
                .or_insert(EnvironmentEntry {
                    value: None,
                    hidden: false,
                });
        } else {
            for (name, value) in matches {
                environment.insert(
                    name,
                    EnvironmentEntry {
                        value: Some(value),
                        hidden: false,
                    },
                );
            }
        }
    }
}
