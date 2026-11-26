use super::*;

struct Queue {
    shared: Arc<Shared>,
    frames: Vec<InsertedQueueFrame<Box<CommandQueueExecution>>>,
    waiting: bool,
    sender: crossbeam_channel::Sender<LeafCompletion>,
    completed: crossbeam_channel::Receiver<LeafCompletion>,
}

type FinishedLeaf = (
    Box<CommandQueueExecution>,
    ExecutionContext,
    InsertedCommandStep,
);

enum LeafCompletion {
    Leaf(Box<FinishedLeaf>),
}

pub(super) struct LoopHooks {
    queues: VecDeque<Queue>,
}

impl LoopHooks {
    pub(super) fn new() -> Self {
        Self {
            queues: VecDeque::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.queues.is_empty()
    }

    pub(super) fn ready(&self) -> bool {
        self.queues.front().is_some_and(|queue| {
            (!queue.waiting
                && queue
                    .shared
                    .command_item
                    .as_ref()
                    .expect("loop hook item")
                    .lock()
                    .pending_wait
                    .as_ref()
                    .is_none_or(|wait| wait.continuation.ready()))
                || !queue.completed.is_empty()
        })
    }

    pub(super) fn events(&mut self, shared: &Arc<Shared>, events: Vec<PendingHookEvent>) {
        self.enqueue(
            shared,
            ExecutionContext::default(),
            &InsertedCommandSource::Events(Box::new(EventQueueSource {
                events: RefCell::new(events),
                publish_control: true,
                shutdown_already_blocked: shared.active_shutdown_blockers() != 0,
            })),
        );
    }

    pub(super) fn shutdown_events(&mut self, shared: &Arc<Shared>, events: Vec<PendingHookEvent>) {
        if events.is_empty() {
            return;
        }
        self.enqueue(
            shared,
            ExecutionContext::default(),
            &InsertedCommandSource::Events(Box::new(EventQueueSource {
                events: RefCell::new(events),
                publish_control: false,
                shutdown_already_blocked: shared.active_shutdown_blockers() != 0,
            })),
        );
    }

    pub(super) fn pending(&self) -> bool {
        !self.queues.is_empty()
    }

    pub(super) fn monitor(
        &mut self,
        shared: &Arc<Shared>,
        context: ExecutionContext,
        commands: Vec<Vec<CommandInvocation>>,
        variables: BTreeMap<String, String>,
    ) {
        self.enqueue(
            shared,
            context,
            &InsertedCommandSource::Hooks(Box::new(HookQueueSource {
                commands: RefCell::new(commands),
                variables,
                skip_resolution_errors: true,
                initial_draining: false,
                replaying: false,
            })),
        );
    }

    #[cfg(feature = "agent")]
    pub(super) fn command(
        &mut self,
        shared: &Arc<Shared>,
        context: ExecutionContext,
        command: CommandInvocation,
    ) {
        if self.enqueue(
            shared,
            context,
            &InsertedCommandSource::Block(RawText::default()),
        ) {
            let queue = self.queues.back_mut().expect("enqueued peer command");
            queue.frames[0].commands = vec![command].into_iter();
        }
    }

    fn enqueue(
        &mut self,
        shared: &Arc<Shared>,
        context: ExecutionContext,
        source: &InsertedCommandSource,
    ) -> bool {
        let shared = shared.command_item(None);
        {
            let mut item = shared.command_item.as_ref().expect("loop hook item").lock();
            item.loop_leaf = true;
            item.loop_wait = true;
        }
        let mut execution = shared.inserted_child_execution(None, false, None);
        execution.item.yield_boundary = true;
        match shared.prepare_inserted_queue_frame(
            context,
            source,
            "<loop-hooks>",
            None,
            MuxOptionSource::RuntimeCommand,
            Box::new(execution),
            None,
            None,
        ) {
            Ok(frame) => {
                let (sender, completed) = crossbeam_channel::unbounded();
                self.queues.push_back(Queue {
                    shared,
                    frames: vec![frame],
                    waiting: false,
                    sender,
                    completed,
                });
                true
            }
            Err(error) => {
                log::debug!("could not prepare queued hook: {error}");
                false
            }
        }
    }

    pub(super) fn turn(
        &mut self,
        _shared: &Arc<Shared>,
        waker: &Arc<mio::Waker>,
    ) -> Result<(), DaemonError> {
        let count = self.queues.len().min(32);
        for _ in 0..count {
            let mut queue = self.queues.pop_front().expect("hook queue");
            queue.turn(waker)?;
            if !queue.frames.is_empty() || queue.waiting {
                self.queues.push_front(queue);
                break;
            }
        }
        Ok(())
    }
}

fn blocking_leaf(command: &CommandInvocation) -> bool {
    matches!(
        canonical_command(&command.name),
        "reload-config" | "import-tmux-config"
    )
}

impl Queue {
    fn turn(&mut self, waker: &Arc<mio::Waker>) -> Result<(), DaemonError> {
        let shared = Arc::clone(&self.shared);
        let client = ClientId(u64::MAX);
        let kind = ClientKind::Command;
        if self.waiting {
            let Ok(completion) = self.completed.try_recv() else {
                return Ok(());
            };
            self.waiting = false;
            shared
                .command_item
                .as_ref()
                .expect("loop hook item")
                .lock()
                .loop_leaf = true;
            match completion {
                LeafCompletion::Leaf(leaf) => {
                    let (execution, context, step) = *leaf;
                    let frame = self.frames.last_mut().expect("waiting frame");
                    frame.execution = execution;
                    frame.context = context;
                    let boundary = frame.parked_boundary.take().expect("parked leaf boundary");
                    if shared
                        .command_item
                        .as_ref()
                        .unwrap()
                        .lock()
                        .pending_wait
                        .is_some()
                    {
                        frame.wait_boundary = Some((boundary, step));
                    } else {
                        let child =
                            shared.settle_inserted_frame_step(client, kind, frame, boundary, step);
                        if let Some(child) = child {
                            self.frames.push(child);
                        }
                    }
                }
            }
        }
        if shared
            .command_item
            .as_ref()
            .unwrap()
            .lock()
            .pending_wait
            .as_ref()
            .is_some_and(|wait| !wait.continuation.ready())
        {
            return Ok(());
        }
        let sender = self.sender.clone();
        let waiting = &mut self.waiting;
        let mut spawn_error = None;
        let finished = shared.advance_inserted_frames(
            client,
            kind,
            &mut self.frames,
            64,
            false,
            false,
            |frame, command, target| {
                if !blocking_leaf(command) {
                    let step =
                        shared.execute_inserted_frame_command(client, kind, frame, command, target);
                    return Some(step);
                }
                if let Some(hook) = &frame.hook {
                    frame.context.no_hooks = true;
                    frame.context.format_variables.clone_from(&hook.variables);
                }
                shared
                    .command_item
                    .as_ref()
                    .expect("loop hook item")
                    .lock()
                    .loop_leaf = false;
                let placeholder = shared.inserted_child_execution(None, false, None);
                let execution = std::mem::replace(&mut frame.execution, Box::new(placeholder));
                let mut context = frame.context.clone();
                let prepared = frame.prepared;
                let mux_source = frame.mux_source;
                let alias_terminal = frame.alias_terminal;
                let command = command.clone();
                let owner = Arc::clone(&shared);
                let sender = sender.clone();
                let wake = Arc::clone(waker);
                if let Err(error) = shared.connection_threads.run(Box::new(move || {
                    let step = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        owner.execute_inserted_leaf(
                            client,
                            kind,
                            &mut context,
                            &command,
                            target,
                            prepared,
                            mux_source,
                            inserted_frame_mode(&execution, alias_terminal),
                        )
                    }))
                    .unwrap_or_else(|_| {
                        (
                            Err(DaemonError::Thread("command leaf panicked".to_owned())),
                            None,
                            false,
                            None,
                            false,
                        )
                    });
                    let _ = sender.send(LeafCompletion::Leaf(Box::new((execution, context, step))));
                    let _ = wake.wake();
                })) {
                    spawn_error = Some(error);
                }
                *waiting = true;
                None
            },
        );
        if let Some(error) = spawn_error {
            return Err(error.into());
        }
        if let Some((finished, result)) = finished {
            shared.finish_command_queue_execution(&finished.execution, None);
            if let Err(error) = result {
                log::debug!("queued hook failed: {error}");
            }
        } else if !self.waiting
            && shared
                .command_item
                .as_ref()
                .unwrap()
                .lock()
                .pending_wait
                .is_none()
        {
            waker.wake()?;
        }
        Ok(())
    }
}

impl Shared {
    pub(super) fn client_lifecycle_hook(self: &Arc<Self>, name: &'static str, client: ClientId) {
        let event = {
            let inner = self.inner.lock();
            client_lifecycle_event(&inner, name, client)
        };
        if let Some(event) = event {
            self.run_event_hooks(vec![event]);
        }
    }

    pub(super) fn finish_hook_leaf_result(
        &self,
        client: ClientId,
        queue_execution: Option<&CommandQueueExecution>,
        result: Result<Execution, DaemonError>,
        pane_exit_code: u8,
        hook_output: &str,
    ) -> Result<Execution, DaemonError> {
        match result {
            Ok(mut execution) => {
                append_inserted_output(&mut execution.output, hook_output);
                self.finish_pane_command(client, execution, pane_exit_code)
            }
            Err(DaemonError::CommandExit {
                mut output,
                exit_code,
            }) => {
                append_inserted_output(&mut output, hook_output);
                Err(DaemonError::CommandExit { output, exit_code })
            }
            Err(DaemonError::ReportedCommandExit { output, .. })
                if let Some(queue_execution) = queue_execution =>
            {
                queue_execution.note_reported_failure();
                Ok(Execution {
                    output,
                    effects: Vec::new(),
                })
            }
            Err(DaemonError::ReportedCommandExit {
                mut output,
                exit_code,
            }) => {
                append_inserted_output(&mut output, hook_output);
                Err(DaemonError::ReportedCommandExit { output, exit_code })
            }
            Err(DaemonError::CommandFailed { mut output, error }) => {
                append_inserted_output(&mut output, hook_output);
                Err(DaemonError::CommandFailed { output, error })
            }
            Err(error) if hook_output.is_empty() => Err(error),
            Err(error) => Err(DaemonError::CommandFailed {
                output: hook_output.into(),
                error: Box::new(error),
            }),
        }
    }

    pub(super) fn command_hook_context(
        &self,
        name: &str,
        execution: &Execution,
        context: &ExecutionContext,
    ) -> ExecutionContext {
        let mut hook_context = context.clone();
        if matches!(
            name,
            "new-session" | "new-window" | "split-window" | "new-pane"
        ) && let Some(pane) = execution.effects.iter().find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        }) {
            let inner = self.inner.lock();
            hook_context.retarget_to_pane(&inner.engine.state, pane);
        }
        hook_context
    }

    pub(super) fn command_hook_set(&self, hook: &str) -> bool {
        self.inner
            .lock()
            .engine
            .has_hook_commands_in_any_session(hook)
    }

    pub(super) fn run_command_hook(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        command: &CommandInvocation,
        hook: &str,
        parent_queue: Option<&CommandQueueExecution>,
    ) -> String {
        let commands = {
            let mut inner = self.inner.lock();
            inner
                .engine
                .note_hook_fired(context.session, hook, unix_timestamp());
            inner
                .engine
                .hook_commands(context.session, hook)
                .filter(|commands| !commands.is_empty())
        };
        let Some(commands) = commands else {
            return String::new();
        };
        self.wake_control_queue(client, kind);
        let mut variables = context.format_variables.clone();
        variables.insert(
            "command".to_owned(),
            canonical_command(&command.name).to_owned(),
        );
        variables.extend(hook_format_variables(command, hook));
        self.run_hook_commands(client, kind, context, commands, &variables, parent_queue)
    }

    pub(super) fn run_hook_commands(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        commands: Vec<Vec<CommandInvocation>>,
        variables: &BTreeMap<String, String>,
        parent_queue: Option<&CommandQueueExecution>,
    ) -> String {
        self.run_hook_commands_with_policy(
            client,
            kind,
            context,
            commands,
            variables,
            false,
            parent_queue,
            false,
        )
        .0
    }

    pub(super) fn run_hook_commands_with_policy(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        commands: Vec<Vec<CommandInvocation>>,
        variables: &BTreeMap<String, String>,
        skip_resolution_errors: bool,
        parent_queue: Option<&CommandQueueExecution>,
        initial_draining: bool,
    ) -> (String, bool) {
        if commands.is_empty() {
            return (String::new(), false);
        }
        let source = InsertedCommandSource::Hooks(Box::new(HookQueueSource {
            commands: RefCell::new(commands),
            variables: variables.clone(),
            skip_resolution_errors,
            initial_draining,
            replaying: context.replay_client().is_some(),
        }));
        if let Some(parent) = parent_queue.filter(|queue| queue.frame_active.get()) {
            parent.insert_foreground_child(InsertedQueueChild {
                context: context.clone(),
                source,
                label: "<hook>".to_owned(),
                control_target: None,
                mux_source: MuxOptionSource::RuntimeCommand,
                kind: InsertedQueueChildKind::Hook(None),
                leaf: None,
                guard: None,
                leaf_name: None,
            });
            return (String::new(), false);
        }
        let mut queue_execution = self.inserted_child_execution(parent_queue, false, None);
        queue_execution.item.yield_boundary = true;
        let mut hook_context = context.clone();
        let result = self.execute_inserted_commands_with_control_target_and_mux_source(
            client,
            kind,
            &mut hook_context,
            &source,
            "<hook>",
            None,
            MuxOptionSource::RuntimeCommand,
            InsertedCommandMode::Standard(&queue_execution),
        );
        let yielded = queue_execution.has_yielded();
        self.finish_command_queue_execution(&queue_execution, parent_queue);
        (
            result.map_or_else(
                |error| daemon_error_output(&error).map_or_else(String::new, ToString::to_string),
                |result| result.output.to_string(),
            ),
            yielded,
        )
    }

    pub(super) fn finish_hook_frame_command<E: FrameExecution>(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        frame: &mut InsertedQueueFrame<E>,
        boundary: InsertedCommandBoundary,
        step: InsertedCommandStep,
    ) -> InsertedFrameAction {
        let policy = frame.hook.as_ref().expect("hook frame policy");
        let variables = &policy.variables;
        let skip_resolution_errors = policy.skip_resolution_errors;
        let replaying = policy.replaying;
        let queue_execution = &frame.execution;
        let control_target = frame.control_target;
        let hook_context = &frame.context;
        let command = boundary.command;
        let callback_failures_start = boundary.callback_failures_start;
        let (execution, _, _, direct_callback_failure, _) = step;
        let output = &mut frame.result.output;
        if let Some(failure) = direct_callback_failure {
            queue_execution
                .callback_parse_failures
                .borrow_mut()
                .push(failure);
        }
        if control_target.is_none()
            && replaying
            && execution.as_ref().err().is_some_and(command_parse_error)
            && queue_execution.callback_parse_failures.borrow().len() == callback_failures_start
        {
            queue_execution.callback_parse_failures.borrow_mut().push(
                CallbackParseFailure::runtime_command(
                    execution
                        .as_ref()
                        .err()
                        .map_or_else(String::new, daemon_error_text),
                    canonical_command(&command.name),
                ),
            );
        }
        let callback_parse_depth = execution
            .as_ref()
            .err()
            .map_or(0, post_admission_callback_parse_depth);
        if callback_parse_depth != 0
            && queue_execution.callback_parse_failures.borrow().len() == callback_failures_start
            && let Some(failure) = CallbackParseFailure::for_command(
                execution
                    .as_ref()
                    .err()
                    .map_or_else(String::new, daemon_error_text),
                canonical_command(&command.name),
                None,
                callback_parse_depth,
            )
        {
            queue_execution
                .callback_parse_failures
                .borrow_mut()
                .push(failure);
        }
        let callback_group_action = queue_execution.callback_group_action_since(
            callback_failures_start,
            canonical_command(&command.name),
            callback_parse_depth,
        );
        let nested_callback_only = callback_parse_depth > 1 && callback_group_action.is_none();
        let mut callback_failures = queue_execution
            .callback_parse_failures
            .borrow_mut()
            .split_off(callback_failures_start);
        if !callback_failures.is_empty() {
            queue_execution.note_reported_failure();
            let owner = if variables
                .get(HOOK_CONTEXT_FORMAT)
                .is_some_and(|hook| hook == "command-error")
            {
                CallbackFailureOwner::CommandErrorHook
            } else {
                CallbackFailureOwner::Hook
            };
            for failure in &mut callback_failures {
                failure.owner = owner;
            }
            if owner == CallbackFailureOwner::CommandErrorHook
                && control_target.is_none()
                && replaying
            {
                queue_execution
                    .deferred_config_replay_issues
                    .borrow_mut()
                    .extend(
                        callback_failures
                            .into_iter()
                            .map(DeferredConfigReplayIssue::Callback),
                    );
            } else {
                queue_execution
                    .callback_parse_failures
                    .borrow_mut()
                    .extend(callback_failures);
            }
        }
        match execution {
            Ok(execution) => {
                if control_target.is_none() {
                    append_inserted_output(output, &execution.output);
                }
            }
            Err(error) => {
                if queue_execution.deferred_shutdown.get() == DeferredShutdown::Force
                    && kind == ClientKind::Command
                    && client != ClientId(u64::MAX)
                {
                    self.record_command_stderr(client, &daemon_error_text(&error));
                }
                if skip_resolution_errors && hook_resolution_error(&error) {
                    return InsertedFrameAction::Finish(None);
                }
                if control_target.is_none() {
                    if let Some(error_output) = daemon_error_output(&error) {
                        append_inserted_output(output, error_output);
                    }
                    self.publish_background_command_error(client, hook_context, &error, false);
                }
                if callback_parse_depth != 0
                    && (callback_group_action == Some(CallbackGroupAction::Continue)
                        || nested_callback_only)
                {
                    return InsertedFrameAction::Continue;
                }
                return InsertedFrameAction::Finish(None);
            }
        }
        InsertedFrameAction::Continue
    }

    pub(super) fn run_event_hooks(self: &Arc<Self>, events: Vec<PendingHookEvent>) {
        if events.is_empty() {
            return;
        }
        #[cfg(any(test, windows))]
        if let Some(effects) = &self.watcher_effects
            && self.timer_rx.lock().is_some()
        {
            effects
                .lock()
                .push(Box::new(move |shared| shared.run_event_hooks(events)));
            return;
        }
        let queued = self.watcher_effects.is_some()
            || self
                .command_item
                .as_ref()
                .is_some_and(|item| item.lock().loop_wait);
        #[cfg(unix)]
        let queued = queued
            || self.loop_leaf_enabled()
            || self.loop_active.load(Ordering::Acquire)
            || self.helpers.on_loop_thread();
        if queued {
            let _ = self.timer_tx.send(timers::TimerInput::Hooks(events));
            return;
        }
        self.run_event_hooks_with_control(events, true);
    }

    pub(super) fn run_shutdown_event_hooks(self: &Arc<Self>, events: Vec<PendingHookEvent>) {
        #[cfg(unix)]
        if self.loop_active.load(Ordering::Acquire) {
            self.lifecycle.hooks(events);
            self.accept_wake.wake();
            return;
        }
        self.run_event_hooks_with_control(events, false);
    }

    fn run_event_hooks_with_control(
        self: &Arc<Self>,
        events: Vec<PendingHookEvent>,
        publish_control: bool,
    ) {
        self.execution_item()
            .run_event_hooks_with_control_in_item(events, publish_control);
    }

    fn run_event_hooks_with_control_in_item(
        self: &Arc<Self>,
        events: Vec<PendingHookEvent>,
        publish_control: bool,
    ) {
        if events.is_empty() {
            return;
        }
        let queue_execution = self.inserted_child_execution(None, false, None);
        let source = InsertedCommandSource::Events(Box::new(EventQueueSource {
            events: RefCell::new(events),
            publish_control,
            shutdown_already_blocked: self.active_shutdown_blockers() != 0,
        }));
        let mut context = ExecutionContext::default();
        let _ = self.execute_inserted_commands_with_control_target_and_mux_source(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &source,
            "<event-hooks>",
            None,
            MuxOptionSource::RuntimeCommand,
            InsertedCommandMode::Standard(&queue_execution),
        );
        self.finish_command_queue_execution(&queue_execution, None);
    }

    pub(super) fn next_event_queue_child(
        self: &Arc<Self>,
        state: &mut EventQueueFrame,
        mut event: PendingHookEvent,
    ) -> Option<InsertedQueueChild> {
        hook_events::fit_payload(event.name, &mut event.variables);
        if state.publish_control && !event.control_notified {
            state
                .notifications
                .push(self.event_control_notification(&event));
        }
        hook_events::complete_client_payload(&self.inner.lock().engine, &mut event);
        self.feed_event_waiters(event.name, &event.variables);
        if self
            .command_item
            .as_ref()
            .is_some_and(|item| item.lock().hook_notifications_only)
        {
            return None;
        }
        let (context, commands) = {
            let mut inner = self.inner.lock();
            let mut context = event.context.clone();
            inner.engine.repair_event_context(&mut context);
            context.set_no_client();
            context.set_replay_client(None);
            context.set_control_command_target(None);
            inner
                .engine
                .note_event_hook_fired(&context, event.name, unix_timestamp());
            let commands = inner
                .engine
                .event_hook_commands(&context, event.name)
                .filter(|commands| !commands.is_empty());
            (context, commands)
        };
        let commands = commands?;
        let draining =
            self.shutdown_pending.load(Ordering::Acquire) && !state.shutdown_already_blocked;
        let mut variables = event.variables;
        variables
            .entry("hook_event".to_owned())
            .or_insert_with(|| event.name.to_owned());
        Some(InsertedQueueChild {
            context,
            source: InsertedCommandSource::Hooks(Box::new(HookQueueSource {
                commands: RefCell::new(commands),
                variables,
                skip_resolution_errors: true,
                initial_draining: draining,
                replaying: false,
            })),
            label: "<event-hook>".to_owned(),
            control_target: None,
            mux_source: MuxOptionSource::RuntimeCommand,
            kind: InsertedQueueChildKind::Event { draining },
            leaf: None,
            guard: None,
            leaf_name: None,
        })
    }

    fn event_control_notification(
        &self,
        event: &PendingHookEvent,
    ) -> (EventPayload, Option<ClientId>, bool) {
        let attached_only = matches!(
            event.name,
            "window-layout-changed"
                | "window-linked"
                | "window-unlinked"
                | "window-renamed"
                | "client-session-changed"
        );
        let mut control_variables = event.variables.clone();
        hook_events::fit_payload(event.name, &mut control_variables);
        if let Some(session) = event.context.session {
            let name = self
                .inner
                .lock()
                .engine
                .state
                .sessions
                .get(&session)
                .map(|session| session.name.clone());
            if event.name == "client-session-changed" {
                control_variables.insert("hook_session".to_owned(), session.to_string());
                if let Some(name) = &name {
                    control_variables.insert("hook_session_name".to_owned(), name.clone());
                }
            }
            control_variables.insert(
                zz_protocol::EVENT_SESSION_VARIABLE.to_owned(),
                session.to_string(),
            );
            if let Some(name) = name {
                control_variables.insert(zz_protocol::EVENT_SESSION_NAME_VARIABLE.to_owned(), name);
            }
        }
        (
            EventPayload::HookEvent {
                name: event.name.to_owned(),
                variables: control_variables,
            },
            event.exclude_client,
            attached_only,
        )
    }

    pub(super) fn publish_event_control_notifications(&self, events: &mut [PendingHookEvent]) {
        for event in events.iter_mut().filter(|event| !event.control_notified) {
            let (payload, exclude_client, attached_only) = self.event_control_notification(event);
            self.publish_to_control_clients(payload, exclude_client, attached_only);
            event.control_notified = true;
        }
    }

    pub(super) fn finish_event_queue_notifications(
        &self,
        control_notifications: Vec<(EventPayload, Option<ClientId>, bool)>,
    ) {
        if let Some(item) = &self.command_item
            && let Some(held) = item.lock().deferred_control_notifications.as_mut()
        {
            held.extend(control_notifications.into_iter().map(
                |(payload, exclude_client, attached_only)| DeferredControlNotification {
                    payload,
                    exclude_client,
                    attached_only,
                },
            ));
            return;
        }
        for (payload, exclude_client, attached_only) in control_notifications {
            self.publish_to_control_clients(payload, exclude_client, attached_only);
        }
    }

    pub(super) fn defer_control_notifications(
        self: &Arc<Self>,
    ) -> DeferredControlNotificationScope {
        self.command_item
            .as_ref()
            .expect("command item")
            .lock()
            .deferred_control_notifications = Some(Vec::new());
        DeferredControlNotificationScope(Arc::clone(self))
    }

    pub(super) fn publish_deferred_control_notifications(self: &Arc<Self>) {
        let held = self
            .command_item
            .as_ref()
            .expect("command item")
            .lock()
            .deferred_control_notifications
            .take()
            .unwrap_or_default();
        for notification in held {
            self.publish_to_control_clients(
                notification.payload,
                notification.exclude_client,
                notification.attached_only,
            );
        }
    }

    pub(super) fn hook_group_execution(
        &self,
        parent: &CommandQueueExecution,
        initial_draining: bool,
    ) -> CommandQueueExecution {
        let mut execution = self.inserted_child_execution(Some(parent), false, None);
        execution.item.draining |= initial_draining;
        execution.item.yield_boundary = true;
        execution
    }
}

fn hook_resolution_error(error: &DaemonError) -> bool {
    match error {
        DaemonError::CommandFailed { error, .. } => hook_resolution_error(error),
        DaemonError::Server(error) => hook_resolution_server_error(error),
        _ => false,
    }
}

fn hook_resolution_server_error(error: &ServerError) -> bool {
    match error {
        ServerError::PostAdmissionCallback(error) => hook_resolution_server_error(error),
        ServerError::MissingTarget(_)
        | ServerError::SessionNotFound(_)
        | ServerError::WindowNotFound(_)
        | ServerError::PaneNotFound(_)
        | ServerError::PaneNotAttached(_)
        | ServerError::PaneExited(_) => true,
        _ => false,
    }
}

pub(super) struct DeferredControlNotificationScope(Arc<Shared>);

impl Drop for DeferredControlNotificationScope {
    fn drop(&mut self) {
        self.0
            .command_item
            .as_ref()
            .expect("command item")
            .lock()
            .deferred_control_notifications
            .take();
    }
}
