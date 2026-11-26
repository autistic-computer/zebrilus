use super::*;

impl MuxEngine {
    pub(super) fn command_prompt(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("command-prompt").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("command-prompt", args)?;
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "command-prompt accepts at most one template".to_owned(),
            ));
        }
        let prompt_type = match options.value("-T") {
            None | Some("command") => CommandPromptType::Command,
            Some("search") => CommandPromptType::Search,
            Some(other) => {
                return Err(ServerError::InvalidCommand(format!(
                    "unknown type: {other}"
                )));
            }
        };
        let mode = command_prompt_mode(&options);
        let template = positional
            .first()
            .map(|template| {
                if invocation.argument_is_command_block(positional_start) {
                    let body = crate::parser::command_block_body(template).unwrap_or(template);
                    self.prepare_frozen_callback_commands(body, true, "command-prompt")
                        .map(CommandPromptTemplate::Commands)
                } else {
                    Ok(CommandPromptTemplate::String(template.to_string()))
                }
            })
            .transpose()?;
        let (labels, pad) = match options.value("-p") {
            Some(labels) => (labels.to_owned(), true),
            None => match &template {
                Some(template) => (
                    format!("({})", command_prompt_template_name(template)),
                    true,
                ),
                None => (":".to_owned(), false),
            },
        };
        let inputs = options.value("-I");
        let steps = if options.has("-l") {
            vec![CommandPromptStep {
                label: labels,
                input: self.expand_prompt_input(context, inputs.unwrap_or_default(), hooks),
            }]
        } else {
            let mut inputs = inputs.map(|inputs| inputs.split(','));
            labels
                .split(',')
                .map(|label| {
                    Ok(CommandPromptStep {
                        label: if pad {
                            format!("{label} ")
                        } else {
                            label.to_owned()
                        },
                        input: self.expand_prompt_input(
                            context,
                            inputs.as_mut().and_then(Iterator::next).unwrap_or_default(),
                            hooks,
                        ),
                    })
                })
                .collect::<Result<Vec<_>, ServerError>>()?
        };
        if steps
            .iter()
            .any(|step| step.label.len() > MAX_COMMAND_PROMPT_LABEL_BYTES)
        {
            return Err(ServerError::InvalidCommand(format!(
                "command prompt label exceeds {MAX_COMMAND_PROMPT_LABEL_BYTES} bytes"
            )));
        }
        if steps
            .iter()
            .any(|step| step.input.len() > zz_protocol::MAX_COMMAND_PROMPT_BYTES)
        {
            return Err(ServerError::InvalidCommand(format!(
                "command prompt input exceeds {} bytes",
                zz_protocol::MAX_COMMAND_PROMPT_BYTES
            )));
        }
        if template.as_ref().is_some_and(|template| match template {
            CommandPromptTemplate::String(template) => {
                template.len() > MAX_COMMAND_PROMPT_TEMPLATE_BYTES
            }
            CommandPromptTemplate::Commands(commands) => {
                format_callback_commands(commands).len() > MAX_COMMAND_PROMPT_TEMPLATE_BYTES
            }
        }) {
            return Err(ServerError::InvalidCommand(format!(
                "command prompt template exceeds {MAX_COMMAND_PROMPT_TEMPLATE_BYTES} bytes"
            )));
        }
        let pane = if options.has("-P") {
            match self.resolve_pane(None, context.window, context.pane) {
                Ok(pane) => Some(pane),
                Err(_) => return Ok(Execution::default()),
            }
        } else {
            None
        };
        Ok(Execution::effect(MuxEffect::CommandPrompt {
            steps,
            template,
            source: invocation.source.clone(),
            prompt_type,
            mode,
            no_freeze: options.has("-C"),
            pane,
        }))
    }

    fn expand_prompt_input(
        &self,
        context: &ExecutionContext,
        input: &str,
        hooks: &mut impl StatusHooks,
    ) -> String {
        let format_context = self
            .resolve_pane(None, context.window, context.pane)
            .ok()
            .and_then(|pane| ExecutionContext::for_pane(&self.state, pane))
            .map_or_else(FormatContext::default, |target| FormatContext {
                session: target.session,
                window: target.window,
                pane: target.pane,
                active_session: None,
                format_client: FormatClient::NoClient,
                format_type: FormatType::Pane,
            });
        expand_format_time_with_hooks(input, self, format_context, hooks).into()
    }

    pub(super) fn display_message(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        stdin: Option<&RawText>,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_display_message_options(args)?;
        let target = self.resolve_display_message_context(context, &options)?;
        let pane = target.as_ref().and_then(|target| target.pane);
        let json = options.has("-j");
        if options.has("-I") && !json {
            let Some(pane) = pane else {
                return Ok(Execution::default());
            };
            return self.pane_stream_input(pane, stdin);
        }
        let format_context = target.map_or_else(FormatContext::default, |target| FormatContext {
            session: target.session,
            window: target.window,
            pane: target.pane,
            active_session: context.session,
            format_client: context.target_format_client(),
            format_type: FormatType::Pane,
        });
        let format = if positional.is_empty() {
            options
                .value("-F")
                .unwrap_or(if json { "" } else { DEFAULT_DISPLAY_MESSAGE })
                .to_owned()
        } else {
            if options.value("-F").is_some() {
                return Err(ServerError::CommandParse(
                    "only one of -F or argument must be given".to_owned(),
                ));
            }
            positional.join(" ")
        };
        let delay = options
            .value("-d")
            .map(|value| parse_strtonum(value, 0, i64::from(u32::MAX), "delay"))
            .transpose()?;
        if options.has("-a") && !json {
            let mut listing = String::new();
            for (name, value) in format_listing(self, format_context, hooks) {
                let _ = writeln!(listing, "{name}={value}");
            }
            return Ok(Execution::output(listing.trim_end_matches('\n').to_owned()));
        }
        let mut trace = Vec::new();
        let text = if options.has("-l") {
            RawText::from(format)
        } else if options.has("-v") {
            let (text, lines) = expand_format_time_traced(&format, self, format_context, hooks);
            trace = lines;
            text
        } else {
            expand_format_time_with_hooks(&format, self, format_context, hooks)
        };
        let text = if json {
            let text = text.to_string();
            crate::layout::json::parse(&text)
                .map(|node| RawText::from(node.to_json()))
                .map_err(ServerError::InvalidCommand)?
        } else {
            text
        };
        if options.has("-p") {
            let mut text = text;
            if text.is_empty() || text.as_bytes().ends_with(b"\n") {
                text.push_bytes(b"\n");
            }
            if trace.is_empty() {
                return Ok(Execution::output(text));
            }
            let mut output = RawText::from(trace.join("\n"));
            output.push_bytes(b"\n");
            output.push_bytes(text.as_bytes());
            return Ok(Execution::output(output));
        }
        if !trace.is_empty() {
            let mut execution = Execution::output(trace.join("\n"));
            execution.effects.push(MuxEffect::DisplayMessage {
                pane,
                target_client: options.value("-c").map(str::to_owned),
                text: text.to_string(),
                duration_ms: delay
                    .map(|delay| u32::try_from(delay).expect("delay was bounded to u32")),
                freeze: !options.has("-C"),
                ignore_keys: options.has("-N"),
            });
            return Ok(execution);
        }
        {
            Ok(Execution::effect(MuxEffect::DisplayMessage {
                pane,
                target_client: options.value("-c").map(str::to_owned),
                text: text.to_string(),
                duration_ms: delay
                    .map(|delay| u32::try_from(delay).expect("delay was bounded to u32")),
                freeze: !options.has("-C"),
                ignore_keys: options.has("-N"),
            }))
        }
    }

    pub fn command_stdin_destination_ready(
        &self,
        context: &ExecutionContext,
        command: &CommandInvocation,
    ) -> Result<bool, ServerError> {
        let name = canonical_command(&command.name);
        if !matches!(name, "display-message" | "split-window") {
            return Ok(true);
        }
        let (options, positional) = parse_command_options(name, &command.args)?;
        if name == "display-message" {
            let target = self.resolve_display_message_context(context, &options)?;
            let Some(pane) = target.and_then(|target| target.pane) else {
                return Ok(false);
            };
            self.pane_stream_input(pane, None)?;
        } else {
            validate_spawn_kind(name, &options, &positional)?;
            self.resolve_pane(options.value("-t"), context.window, context.pane)?;
            pane_spawn_empty(&options, shell_command_positional(&positional).as_deref())?;
            self.split_placement(&options, split_size(&options))?;
        }
        Ok(true)
    }

    fn pane_stream_input(
        &self,
        pane: PaneId,
        stdin: Option<&RawText>,
    ) -> Result<Execution, ServerError> {
        if !self.state.pane(pane).is_some_and(|state| state.empty) {
            return Err(ServerError::InvalidCommand("pane is not empty".to_owned()));
        }
        Ok(stdin.map_or_else(Execution::default, |bytes| {
            Execution::effect(MuxEffect::PaneStreamInput {
                pane,
                bytes: bytes.clone(),
            })
        }))
    }

    pub fn display_message_format_target(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<(Option<ExecutionContext>, Option<String>), ServerError> {
        let (options, _) = parse_command_options("display-message", args)?;
        Ok((
            self.resolve_display_message_context(context, &options)?,
            options.value("-c").map(str::to_owned),
        ))
    }

    fn resolve_display_message_context(
        &self,
        context: &ExecutionContext,
        options: &Options,
    ) -> Result<Option<ExecutionContext>, ServerError> {
        match options.value("-t") {
            Some(target) => match self.resolve_pane(Some(target), context.window, context.pane) {
                Ok(pane) => Ok(ExecutionContext::for_pane(&self.state, pane)),
                Err(
                    ServerError::SessionNotFound(_)
                    | ServerError::WindowNotFound(_)
                    | ServerError::PaneNotFound(_),
                ) => Ok(self.resolve_display_message_partial_context(context, target)),
                Err(ServerError::InvalidCommand(message)) if message == NO_MARKED_TARGET => {
                    Ok(None)
                }
                Err(error) => Err(error),
            },
            None if self.state.sessions.is_empty() => Ok(None),
            None => self
                .resolve_pane(None, context.window, context.pane)
                .map(|pane| ExecutionContext::for_pane(&self.state, pane)),
        }
    }

    fn resolve_display_message_partial_context(
        &self,
        context: &ExecutionContext,
        target: &str,
    ) -> Option<ExecutionContext> {
        let window = if let Some((session_target, window_and_pane)) = target.split_once(':') {
            let session = self
                .resolve_session(
                    (!session_target.is_empty()).then_some(session_target),
                    context.session,
                )
                .ok()?;
            let active_window = self.state.sessions.get(&session)?.active_window;
            let window_target = window_and_pane
                .split_once('.')
                .map_or(window_and_pane, |(window, _)| window);
            if window_target.is_empty() {
                active_window
            } else {
                let target = format!("{session}:{window_target}");
                self.resolve_window(Some(&target), Some(session), context.window)
                    .unwrap_or(active_window)
            }
        } else {
            let (window_target, _) = target.split_once('.')?;
            if is_marked_target(window_target) {
                return None;
            }
            self.resolve_window(
                (!window_target.is_empty()).then_some(window_target),
                context.session,
                context.window,
            )
            .ok()?
        };
        let window_state = self.state.windows.get(&window)?;
        Some(ExecutionContext::new(
            Some(window_state.session),
            Some(window),
            Some(window_state.active_pane),
        ))
    }
}

fn command_prompt_template_name(template: &CommandPromptTemplate) -> &str {
    match template {
        CommandPromptTemplate::Commands(commands) => commands
            .first()
            .map_or("", |command| canonical_command(&command.name)),
        CommandPromptTemplate::String(template) => {
            template.split([' ', ',']).next().unwrap_or(template)
        }
    }
}

fn command_prompt_mode(options: &Options) -> CommandPromptMode {
    if options.has("-1") {
        CommandPromptMode::Single
    } else if options.has("-N") {
        CommandPromptMode::Numeric
    } else if options.has("-i") {
        CommandPromptMode::Incremental
    } else if options.has("-k") {
        CommandPromptMode::Key
    } else if options.has("-e") {
        CommandPromptMode::BackspaceExit
    } else {
        CommandPromptMode::Text
    }
}

pub(super) fn parse_display_message_options(
    args: &[RawText],
) -> Result<(Options, Vec<RawText>), ServerError> {
    match args {
        [print, payload] if print == "-p" && !payload.starts_with('-') => Ok((
            Options {
                flags: vec!["-p".to_owned()],
                values: Vec::new(),
            },
            vec![payload.clone()],
        )),
        _ => parse_command_options("display-message", args),
    }
}
