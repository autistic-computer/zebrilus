use super::*;

#[derive(Default)]
pub(super) struct Load {
    pub(super) source: Option<(PathBuf, usize)>,
    waiters: Vec<Arc<terminal_requests::CommandState>>,
    loading: bool,
}

#[derive(Default)]
pub(super) struct Saves {
    pending: VecDeque<Save>,
    running: bool,
}

struct Save {
    path: PathBuf,
    command: Vec<String>,
    search: Vec<String>,
    done: Option<Arc<terminal_requests::CommandState>>,
}

impl Shared {
    pub(super) fn ensure_prompt_history(self: &Arc<Self>) {
        if self.prompt_history_settled.load(Ordering::Acquire) {
            return;
        }
        let wait = terminal_requests::CommandWait::new(self);
        let state = wait.start();
        {
            let mut load = self.prompt_history_source.lock();
            if self.prompt_history_settled.load(Ordering::Acquire) {
                state.resolve(Ok(Execution::default()));
            } else {
                load.waiters.push(state);
                if let Some((path, limit)) = load.source.take() {
                    load.loading = true;
                    if let Err(error) = self.submit_helper(helpers::Task::HistoryLoad {
                        path: path.clone(),
                        limit,
                        complete: Box::new(|shared, (command, search)| {
                            let waiters = {
                                let mut load = shared.prompt_history_source.lock();
                                load.loading = false;
                                let mut inner = shared.inner.lock();
                                inner.history.command_history = command;
                                inner.history.search_history = search;
                                shared.prompt_history_settled.store(true, Ordering::Release);
                                std::mem::take(&mut load.waiters)
                            };
                            for waiter in waiters {
                                waiter.resolve(Ok(Execution::default()));
                            }
                        }),
                    }) {
                        load.loading = false;
                        load.source = Some((path, limit));
                        log::warn!("could not load prompt history: {error}");
                        for waiter in load.waiters.drain(..) {
                            waiter.resolve(Err(std::io::Error::other(error.to_string()).into()));
                        }
                    }
                } else if !load.loading {
                    self.prompt_history_settled.store(true, Ordering::Release);
                    for waiter in load.waiters.drain(..) {
                        waiter.resolve(Ok(Execution::default()));
                    }
                }
            }
        }
        let _ = wait.finish(self, Execution::default());
    }

    pub(super) fn persist_prompt_history_with_observer<BeforeLock, AfterLock>(
        self: &Arc<Self>,
        before_lock: BeforeLock,
        after_lock: AfterLock,
    ) where
        BeforeLock: FnOnce(),
        AfterLock: FnOnce(),
    {
        before_lock();
        let queued = self.command_item.as_ref().is_some_and(|item| {
            let item = item.lock();
            item.loop_wait || {
                #[cfg(unix)]
                {
                    item.loop_leaf
                }
                #[cfg(not(unix))]
                {
                    false
                }
            }
        });
        let wait = (!self.helpers.on_loop_thread() || queued)
            .then(|| terminal_requests::CommandWait::new(self));
        let done = wait.as_ref().map(terminal_requests::CommandWait::start);
        {
            let mut saves = self.prompt_history_effects.lock();
            after_lock();
            let save = {
                let inner = self.inner.lock();
                prompt_history_path(inner.engine.history_file()).map(|path| Save {
                    path,
                    command: inner.history.command_history.clone(),
                    search: inner.history.search_history.clone(),
                    done: done.clone(),
                })
            };
            if let Some(save) = save {
                saves.pending.push_back(save);
                self.start_prompt_history_save(&mut saves);
            } else if let Some(done) = &done {
                done.resolve(Ok(Execution::default()));
            }
        }
        if let Some(wait) = wait {
            let _ = wait.finish(self, Execution::default());
        }
    }

    fn start_prompt_history_save(self: &Arc<Self>, saves: &mut Saves) {
        if saves.running {
            return;
        }
        while let Some(save) = saves.pending.pop_front() {
            let failed = save.done.clone();
            saves.running = true;
            let submitted = self.submit_helper(helpers::Task::HistorySave {
                path: save.path,
                command: save.command,
                search: save.search,
                complete: Box::new(move |shared, ()| {
                    {
                        let mut saves = shared.prompt_history_effects.lock();
                        saves.running = false;
                        shared.start_prompt_history_save(&mut saves);
                    }
                    if let Some(done) = save.done {
                        done.resolve(Ok(Execution::default()));
                    }
                }),
            });
            if let Err(error) = submitted {
                saves.running = false;
                log::warn!("could not save prompt history: {error}");
                if let Some(failed) = failed {
                    failed.resolve(Err(error.into()));
                }
            } else {
                break;
            }
        }
    }
}

impl Shared {
    pub(super) fn publish_status_message(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        text: String,
    ) {
        let (publication, retired, deadline) = {
            let mut inner = self.inner.lock();
            let client_name = server_log_client_name(&inner, client);
            push_server_message(&mut inner, format!("{client_name} message: {text}"));
            let duration_ms = client_attached_session(&inner, client).map_or(750, |session| {
                inner.engine.display_time_for_session(session)
            });
            let message_id = next_timed_message_id(&mut inner);
            if duration_ms != 0 {
                let _ = inner
                    .client_mut(client)
                    .is_some_and(|client| std::mem::take(&mut client.message_ignore_keys));
            }
            let freeze = !inner.client(client).is_some_and(|c| c.native_chooser);
            let (retired, deadline) =
                arm_client_message(&mut inner, client, message_id, duration_ms, freeze);
            let publication = OwnedClientMessagePublication {
                client,
                pane: context.pane,
                kind: ClientMessageKind::Error,
                text,
                duration_ms,
                message_id,
            };
            (publication, retired, deadline)
        };
        self.publish_owned_client_message(publication);
        if let Some(retired) = retired {
            self.retire_client_message(client, retired, true);
        }
        if let Some(deadline) = deadline {
            let _ = self.timer_tx.send(timers::TimerInput::ClientMessage(
                ClientMessageDeadlineCommand::Schedule(deadline),
            ));
        }
    }

    pub(super) fn show_messages(
        &self,
        name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_buffer_command_args(name, args, &['t'], &['J', 'T'])?;
        require_no_positionals(name, &parsed)?;
        if parsed.has('T') || parsed.has('J') {
            let mut lines = Vec::new();
            if parsed.has('T') {
                let inner = self.inner.lock();
                let target = parsed
                    .value('t')
                    .and_then(|target| find_attached_client_with_aliases(&inner, target, true));
                lines = terminal_descriptions(&inner, parsed.value('t').is_some(), target);
            }
            if parsed.has('J') {
                let summaries = self.status.lock().job_summaries();
                if !lines.is_empty() && !summaries.is_empty() {
                    lines.push(String::new());
                }
                lines.extend(summaries);
            }
            return Ok(Execution {
                output: lines.join("\n").into(),
                effects: Vec::new(),
            });
        }
        let mut inner = self.inner.lock();
        inner.engine.set_format_now(unix_timestamp());
        let context =
            server_format_context(&inner.engine, &inner.config.config_files, None, None, None);
        let output = inner
            .history
            .message_log
            .iter()
            .rev()
            .map(|message| {
                let facts = FormatHookFacts {
                    message: Some(MessageFormatFacts {
                        number: message.number,
                        text: message.text.clone(),
                        time: message.time,
                    }),
                    ..FormatHookFacts::default()
                };
                let mut hooks =
                    DaemonFormatHooks::command(&facts).with_option_engine(&inner.engine);
                expand_format_values("#{t/p:message_time}: #{message_text}", &context, &mut hooks)
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Execution {
            output: output.into(),
            effects: Vec::new(),
        })
    }

    pub(super) fn prompt_history_command(
        self: &Arc<Self>,
        name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        self.ensure_prompt_history();
        let parsed = parse_buffer_command_args(name, args, &['T'], &[])?;
        require_no_positionals(name, &parsed)?;
        let prompt_type = parsed
            .value('T')
            .map(|value| match value {
                "command" => Ok(CommandPromptType::Command),
                "search" => Ok(CommandPromptType::Search),
                _ => Err(ServerError::InvalidCommand(format!(
                    "invalid type: {value}"
                ))),
            })
            .transpose()?;

        if name == "clear-prompt-history" {
            self.clear_prompt_history(prompt_type);
            return Ok(Execution::default());
        }

        let inner = self.inner.lock();
        let mut output = String::new();
        match prompt_type {
            Some(CommandPromptType::Command) => {
                append_prompt_history_output(
                    &mut output,
                    "command",
                    &inner.history.command_history,
                );
            }
            Some(CommandPromptType::Search) => {
                append_prompt_history_output(&mut output, "search", &inner.history.search_history);
            }
            None => {
                append_prompt_history_output(
                    &mut output,
                    "command",
                    &inner.history.command_history,
                );
                append_prompt_history_output(&mut output, "search", &inner.history.search_history);
            }
        }
        Ok(Execution {
            output: output.into(),
            effects: Vec::new(),
        })
    }

    pub(super) fn record_prompt_history(
        self: &Arc<Self>,
        prompt_type: CommandPromptType,
        input: &str,
    ) {
        self.record_prompt_history_with_persist_observer(prompt_type, input, || {}, || {});
    }

    pub(super) fn record_prompt_history_with_persist_observer<BeforeLock, AfterLock>(
        self: &Arc<Self>,
        prompt_type: CommandPromptType,
        input: &str,
        before_lock: BeforeLock,
        after_lock: AfterLock,
    ) where
        BeforeLock: FnOnce(),
        AfterLock: FnOnce(),
    {
        if input.is_empty() {
            return;
        }
        self.ensure_prompt_history();
        let changed = {
            let mut inner = self.inner.lock();
            let limit = inner.engine.prompt_history_limit();
            let history = match prompt_type {
                CommandPromptType::Command => &mut inner.history.command_history,
                CommandPromptType::Search => &mut inner.history.search_history,
            };
            add_command_prompt_history(history, input, limit)
        };
        if changed {
            self.persist_prompt_history_with_observer(before_lock, after_lock);
        }
    }

    fn clear_prompt_history(self: &Arc<Self>, prompt_type: Option<CommandPromptType>) {
        self.clear_prompt_history_with_persist_observer(prompt_type, || {}, || {});
    }

    pub(super) fn clear_prompt_history_with_persist_observer<BeforeLock, AfterLock>(
        self: &Arc<Self>,
        prompt_type: Option<CommandPromptType>,
        before_lock: BeforeLock,
        after_lock: AfterLock,
    ) where
        BeforeLock: FnOnce(),
        AfterLock: FnOnce(),
    {
        {
            let mut inner = self.inner.lock();
            match prompt_type {
                Some(CommandPromptType::Command) => inner.history.command_history.clear(),
                Some(CommandPromptType::Search) => inner.history.search_history.clear(),
                None => {
                    inner.history.command_history.clear();
                    inner.history.search_history.clear();
                }
            }
        }
        self.persist_prompt_history_with_observer(before_lock, after_lock);
    }

    pub(super) fn retire_client_message(
        self: &Arc<Self>,
        client: ClientId,
        retired: ActiveClientMessage,
        notify: bool,
    ) {
        if retired.deadline.is_some() {
            let _ = self.timer_tx.send(timers::TimerInput::ClientMessage(
                ClientMessageDeadlineCommand::Cancel {
                    client,
                    token: retired.token,
                },
            ));
        }
        if notify {
            self.publish_to_client(
                client,
                EventPayload::TimedClientMessageCleared {
                    message_id: retired.token,
                },
            );
        }
        if retired.freeze {
            self.resume_client_terminals(client);
        }
    }

    pub(super) fn expire_client_message(
        self: &Arc<Self>,
        scheduled: ClientMessageDeadline,
        now: Instant,
    ) {
        let retired = {
            let mut inner = self.inner.lock();
            let due = inner
                .client(scheduled.client)
                .and_then(|c| c.message.as_ref())
                .is_some_and(|current| {
                    current.token == scheduled.token
                        && current.deadline == Some(scheduled.deadline)
                        && scheduled.deadline <= now
                });
            if !due {
                return;
            }
            take_client_message(&mut inner, scheduled.client)
        };
        if let Some(retired) = retired {
            self.retire_client_message(scheduled.client, retired, true);
        }
    }

    pub(super) fn dismiss_client_message(self: &Arc<Self>, client: ClientId) {
        let Some(retired) = ({
            let mut inner = self.inner.lock();
            take_client_message(&mut inner, client)
        }) else {
            return;
        };
        self.retire_client_message(client, retired, true);
    }
}

fn append_prompt_history_output(output: &mut String, prompt_type: &str, history: &[String]) {
    let _ = writeln!(output, "History for {prompt_type}:\n");
    for (index, entry) in history.iter().enumerate() {
        let _ = writeln!(output, "{}: {entry}", index + 1);
    }
    output.push('\n');
}

fn client_negotiated_features(inner: &ServerState, client: ClientId) -> String {
    client_negotiated_features_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn terminal_descriptions(
    inner: &ServerState,
    targeted: bool,
    target: Option<ClientId>,
) -> Vec<String> {
    let features = inner.engine.terminal_features_option();
    let overrides = inner.engine.terminal_overrides_option();
    let mut lines = Vec::new();
    let mut number = 0;
    for client in inner
        .clients
        .iter()
        .rev()
        .filter_map(|(id, client)| client.has_terminal.then_some(*id))
    {
        if targeted && target.is_some_and(|target| target != client) {
            continue;
        }
        let term = client_environment_value(inner, client, "TERM").unwrap_or_default();
        let Some(terminal) = client_terminal_facts(
            term,
            client_environment_value(inner, client, "COLORTERM"),
            &client_negotiated_features(inner, client),
            &features,
            &overrides,
        ) else {
            continue;
        };
        lines.push(format!(
            "Terminal {number}: {term} for {}, flags=0x{:x}:",
            client_format_name(inner, client),
            terminal.flags()
        ));
        lines.extend(terminal.describe());
        number += 1;
    }
    lines
}
