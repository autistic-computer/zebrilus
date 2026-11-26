use super::*;

impl Shared {
    pub(super) fn capture_pane(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        command_name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let mut parsed = parse_capture_pane_args(args)?;
        let chat = {
            let inner = self.inner.lock();
            let pane = inner.engine.resolve_pane(
                parsed.target.as_deref(),
                context.window,
                context.pane,
            )?;
            is_agent_chat(&inner, pane).then_some(pane)
        };
        if let Some(pane) = chat {
            #[cfg(feature = "agent")]
            let text = match self.open_agent_runtime() {
                Some(runtime) => {
                    if runtime.wire_state(pane).is_some_and(|state| {
                        matches!(
                            state.phase,
                            zz_protocol::AgentConnectionPhase::Running
                                | zz_protocol::AgentConnectionPhase::AwaitingPermission
                        )
                    }) {
                        self.record_command_stderr(
                            client,
                            &format!("capture-pane: {pane}'s turn is still running"),
                        );
                    }
                    runtime.chat_text(pane, parsed.start.is_some())
                }
                None => String::new(),
            };
            #[cfg(not(feature = "agent"))]
            let text = String::new();
            if parsed.print && text.is_empty() {
                return Ok(Execution::default());
            }
            return self.finish_capture_pane(pane, &parsed, context.no_hooks, Ok(text));
        }
        let (pane, terminal, mut start, mut end) = {
            let mut inner = self.inner.lock();
            let pane = inner.engine.resolve_pane(
                parsed.target.as_deref(),
                context.window,
                context.pane,
            )?;
            inner.engine.set_format_now(unix_timestamp());
            let target = ExecutionContext::for_pane(&inner.engine.state, pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            let facts = borrowed_format_hook_facts(&inner);
            let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                &facts,
                context.format_variables(),
            )
            .with_command_item(command_name);
            let start = resolve_capture_boundary(
                &inner.engine,
                parsed.start.as_deref(),
                true,
                &target,
                context.session,
                context.target_format_client(),
                &mut hooks,
            );
            let end = resolve_capture_boundary(
                &inner.engine,
                parsed.end.as_deref(),
                false,
                &target,
                context.session,
                context.target_format_client(),
                &mut hooks,
            );
            let terminal = inner
                .terminals
                .get(&pane)
                .cloned()
                .ok_or(ServerError::PaneExited(pane))?;
            (pane, terminal, start, end)
        };
        if matches!(
            (start, end),
            (CaptureBoundary::Relative(start), CaptureBoundary::Relative(end)) if start > end
        ) {
            std::mem::swap(&mut start, &mut end);
        }
        parsed.options.start = start;
        parsed.options.end = end;
        let wait = terminal_requests::CommandWait::new(self);
        let request = terminal.capture_request(parsed.options, self.terminal_requests.notifier());
        let no_hooks = context.no_hooks;
        let plain = !(parsed.options.escape_sequences
            || parsed.options.escape_nonprintable
            || parsed.options.number_lines
            || parsed.options.line_flags
            || parsed.options.hyperlinks);
        let retained = Arc::clone(&terminal);
        wait.read(
            self,
            client,
            pane,
            terminal,
            request,
            move |shared, result| {
                let capture = result
                    .map_err(TerminalCaptureError::from)
                    .and_then(|result| result);
                let capture = if matches!(capture, Err(TerminalCaptureError::ActorStopped))
                    && shared
                        .inner
                        .lock()
                        .engine
                        .state
                        .pane(pane)
                        .is_some_and(|pane| pane.dead)
                {
                    retained.capture_retained_frame(parsed.options)
                } else {
                    capture
                };
                let capture = capture.map(|text| {
                    if plain {
                        terminal_reads::without_run_pane_lines(
                            &text,
                            usize::from(retained.size().0),
                        )
                    } else {
                        text
                    }
                });
                shared.finish_capture_pane(pane, &parsed, no_hooks, capture)
            },
        );
        wait.finish(self, Execution::default())
    }

    fn finish_capture_pane(
        self: &Arc<Self>,
        pane: PaneId,
        parsed: &ParsedCapturePane,
        no_hooks: bool,
        capture: Result<String, TerminalCaptureError>,
    ) -> Result<Execution, DaemonError> {
        let mut unavailable_alternate = false;
        let output = match capture {
            Ok(output) => output,
            Err(TerminalCaptureError::AlternateUnavailable) if parsed.quiet => {
                unavailable_alternate = true;
                String::new()
            }
            Err(TerminalCaptureError::ActorStopped) => {
                return Err(ServerError::PaneExited(pane).into());
            }
            Err(TerminalCaptureError::TimedOut) => {
                return Err(ServerError::Internal("terminal capture timed out".to_owned()).into());
            }
            Err(TerminalCaptureError::Failed(error)) => {
                return Err(
                    ServerError::Internal(format!("terminal capture failed: {error}")).into(),
                );
            }
            Err(error) => return Err(ServerError::InvalidCommand(error.to_string()).into()),
        };
        if parsed.print {
            let mut printed = output;
            printed.push('\n');
            return Ok(Execution {
                output: printed.into(),
                effects: Vec::new(),
            });
        }
        let no_links = parsed.options.hyperlinks && output.is_empty();
        let mut data = output.into_bytes();
        if !unavailable_alternate && !no_links {
            data.push(b'\n');
        }
        let events = {
            let mut inner = self.inner.lock();
            insert_paste_buffer(
                &mut inner,
                parsed.buffer_name.as_deref(),
                "buffer",
                data,
                true,
            )?
        };
        if !no_hooks {
            self.run_event_hooks(events);
        }
        self.refresh_choose_buffers();
        Ok(Execution::default())
    }
}

#[derive(Debug)]
pub(super) struct ParsedCapturePane {
    pub(super) target: Option<String>,
    pub(super) buffer_name: Option<String>,
    pub(super) start: Option<String>,
    pub(super) end: Option<String>,
    pub(super) options: CaptureOptions,
    pub(super) print: bool,
    quiet: bool,
}

pub(super) fn parse_capture_pane_args(args: &[RawText]) -> Result<ParsedCapturePane, ServerError> {
    let args = parse_buffer_command_args(
        "capture-pane",
        args,
        &['b', 'E', 'S', 't'],
        &['a', 'C', 'e', 'F', 'H', 'J', 'L', 'M', 'N', 'p', 'T', 'q'],
    )?;
    require_no_positionals("capture-pane", &args)?;
    let options = CaptureOptions {
        alternate: args.has('a'),
        escape_sequences: args.has('e'),
        escape_nonprintable: args.has('C'),
        join_wrapped: args.has('J'),
        number_lines: args.has('L'),
        line_flags: args.has('F'),
        hyperlinks: args.has('H'),
        mode: args.has('M'),
        preserve_trailing: args.has('J') || args.has('N'),
        trim_positions: args.has('T'),
        ..CaptureOptions::default()
    };
    let parsed = ParsedCapturePane {
        target: args.value('t').map(str::to_owned),
        buffer_name: args.value('b').map(str::to_owned),
        start: args.value('S').map(str::to_owned),
        end: args.value('E').map(str::to_owned),
        options,
        print: args.has('p'),
        quiet: args.has('q'),
    };
    Ok(parsed)
}

pub(super) fn resolve_capture_boundary(
    engine: &MuxEngine,
    value: Option<&str>,
    start: bool,
    target: &ExecutionContext,
    active_session: Option<SessionId>,
    format_client: FormatClient,
    hooks: &mut impl StatusHooks,
) -> CaptureBoundary {
    let Some(value) = value else {
        return default_capture_boundary(start);
    };
    if value == "-" {
        return if start {
            CaptureBoundary::HistoryStart
        } else {
            CaptureBoundary::VisibleEnd
        };
    }
    let expanded = engine.expand_pane_format(value, target, active_session, format_client, hooks);
    expanded
        .parse::<i64>()
        .ok()
        .filter(|line| *line >= i64::from(i32::MIN) && *line <= i64::from(i16::MAX))
        .map_or_else(
            || default_capture_boundary(start),
            CaptureBoundary::Relative,
        )
}

fn default_capture_boundary(start: bool) -> CaptureBoundary {
    if start {
        CaptureBoundary::Relative(0)
    } else {
        CaptureBoundary::VisibleEnd
    }
}
