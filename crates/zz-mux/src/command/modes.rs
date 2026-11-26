use super::*;

impl MuxEngine {
    pub(super) fn copy_mode(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("copy-mode", args)?;
        reject_positionals("copy-mode", &positional)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        if options.has("-q") {
            return Ok(Execution {
                output: RawText::default(),
                effects: vec![
                    MuxEffect::PaneModeChanged { pane, mode: None },
                    MuxEffect::TerminalView {
                        pane,
                        action: TerminalViewAction::CopyMode(CopyModeAction::Cancel),
                        target_client: None,
                        require_mode: false,
                    },
                ],
            });
        }
        if options.has("-M") {
            let Some(mouse) = context.invoking_mouse() else {
                return Ok(Execution::default());
            };
            let Some(target) = mouse.pane else {
                return Ok(Execution::default());
            };
            let mut effects = vec![MuxEffect::TerminalView {
                pane: target,
                action: TerminalViewAction::EnterCopyModeWith {
                    scroll_exit: false,
                    hide_position: true,
                },
                target_client: None,
                require_mode: false,
            }];
            effects.extend(
                [mouse.press_action.clone(), mouse.view_action.clone()]
                    .into_iter()
                    .flatten()
                    .map(|action| MuxEffect::TerminalView {
                        pane: target,
                        action,
                        target_client: None,
                        require_mode: true,
                    }),
            );
            return Ok(Execution {
                output: RawText::default(),
                effects,
            });
        }
        let mut effects = Vec::new();
        if let Some(source) = options.value("-s") {
            let source = self.resolve_pane(Some(source), context.window, context.pane)?;
            if source != pane {
                effects.push(MuxEffect::ArmCopyModeSource { pane, source });
            }
        }
        if options.has("-k") {
            effects.push(MuxEffect::ArmCopyModeKill { pane });
        }
        let hide_position = options.has("-H") || context.invoking_mouse().is_some();
        effects.push(MuxEffect::TerminalView {
            pane,
            action: if hide_position {
                TerminalViewAction::EnterCopyModeWith {
                    scroll_exit: options.has("-e"),
                    hide_position: true,
                }
            } else if options.has("-e") {
                TerminalViewAction::EnterCopyModeScrollExit
            } else {
                TerminalViewAction::EnterCopyMode
            },
            target_client: None,
            require_mode: false,
        });
        if options.has("-u") {
            effects.push(MuxEffect::TerminalView {
                pane,
                action: TerminalViewAction::CopyMode(CopyModeAction::PageUp),
                target_client: None,
                require_mode: false,
            });
        }
        if options.has("-d") {
            effects.push(MuxEffect::TerminalView {
                pane,
                action: TerminalViewAction::CopyMode(if options.has("-e") {
                    CopyModeAction::PageDownScrollExit
                } else {
                    CopyModeAction::PageDown
                }),
                target_client: None,
                require_mode: false,
            });
        }
        Ok(Execution {
            output: RawText::default(),
            effects,
        })
    }

    pub(super) fn copy_mode_search_prompt(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, _) = parse_command_options("copy-mode-search-prompt", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::TerminalUi {
            pane,
            command: TerminalUiCommand::BeginSearch {
                direction: if options.has("-b") {
                    SearchDirection::Backward
                } else {
                    SearchDirection::Forward
                },
            },
        }))
    }

    pub(super) fn clear_history(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, _) = parse_command_options("clear-history", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::TerminalView {
            pane,
            action: TerminalViewAction::ClearHistory,
            target_client: None,
            require_mode: false,
        }))
    }

    pub(super) fn clock_mode(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("clock-mode", args)?;
        reject_positionals("clock-mode", &positional)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::PaneModeChanged {
            pane,
            mode: Some(PaneModeRequest::Clock),
        }))
    }

    pub(super) fn switch_mode(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("switch-mode").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("switch-mode", args)?;
        spec.validate_positional_maximum(positional.len())?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::PaneModeChanged {
            pane,
            mode: Some(PaneModeRequest::Switch(Box::new(
                SwitchMode::new(
                    options.has("-w"),
                    options.value("-F").map(str::to_owned),
                    chooser_command_template(invocation, positional_start, &positional),
                    options.has("-k"),
                    options.has("-Z"),
                    self.word_separators_for_pane(pane)?,
                )
                .with_status_keys(self.pane_prompt_keys_are_vi(pane)),
            ))),
        }))
    }

    pub(super) fn choose_tree(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("choose-tree").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("choose-tree", args)?;
        spec.validate_positional_maximum(positional.len())?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let sort = TmuxSort::parse(
            options.value("-O"),
            options.has("-r"),
            Some(TmuxSortOrder::Index),
        )?;
        Ok(Execution::effect(MuxEffect::ChooseTree {
            pane,
            preview: chooser_preview(&options),
            kind: if options.has("-s") || options.has("-w") {
                ChooseTreeKind::Windows
            } else {
                ChooseTreeKind::Panes
            },
            info_preview: false,
            sessions_only: options.has("-s"),
            filter: options.value("-f").map(str::to_owned),
            format: options.value("-F").map(str::to_owned),
            hide_source: options.has("-h"),
            kill_source: options.has("-k"),
            prompt_accept: options.has("-y"),
            sort,
            key_format: options.value("-K").map(str::to_owned),
            template: chooser_command_template(invocation, positional_start, &positional),
            zoom: options.has("-Z"),
        }))
    }

    pub(super) fn choose_client(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("choose-client").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("choose-client", args)?;
        spec.validate_positional_maximum(positional.len())?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let sort = TmuxSort::parse(
            options.value("-O"),
            options.has("-r"),
            Some(TmuxSortOrder::Name),
        )?;
        Ok(Execution::effect(MuxEffect::ChooseTree {
            pane,
            preview: chooser_preview(&options),
            kind: ChooseTreeKind::Clients,
            info_preview: options.has("-i"),
            sessions_only: false,
            filter: options.value("-f").map(str::to_owned),
            format: options.value("-F").map(str::to_owned),
            hide_source: options.has("-h"),
            kill_source: options.has("-k"),
            prompt_accept: options.has("-y"),
            sort,
            key_format: options.value("-K").map(str::to_owned),
            template: chooser_command_template(invocation, positional_start, &positional),
            zoom: options.has("-Z"),
        }))
    }

    pub(super) fn focus_sidebar(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("focus-sidebar", args)?;
        if !positional.is_empty() {
            return Err(ServerError::CommandParse(
                "focus-sidebar does not take positional arguments".to_owned(),
            ));
        }
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::FocusSidebar { pane }))
    }

    pub(super) fn choose_path(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("choose-path", args)?;
        if !positional.is_empty() {
            return Err(ServerError::CommandParse(
                "choose-path does not take positional arguments".to_owned(),
            ));
        }
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let start_dir = options.value("-c").and_then(|value| {
            let format_context = ExecutionContext::for_pane(&self.state, pane).map_or_else(
                FormatContext::default,
                |origin| FormatContext {
                    session: origin.session,
                    window: origin.window,
                    pane: origin.pane,
                    active_session: origin.session,
                    format_client: context.format_client(),
                    format_type: FormatType::Pane,
                },
            );
            let expanded = expand_format_with_hooks(value, self, format_context, hooks);
            (!expanded.is_empty()).then(|| expanded.to_string())
        });
        Ok(Execution::effect(MuxEffect::ChoosePath { pane, start_dir }))
    }

    pub(super) fn choose_buffer(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("choose-buffer").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("choose-buffer", args)?;
        spec.validate_positional_maximum(positional.len())?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let sort = TmuxSort::parse(
            options.value("-O"),
            options.has("-r"),
            Some(TmuxSortOrder::Creation),
        )?;
        Ok(Execution::effect(MuxEffect::ChooseBuffer {
            pane,
            preview: chooser_preview(&options),
            filter: options.value("-f").map(str::to_owned),
            format: options.value("-F").map(str::to_owned),
            kill_source: options.has("-k"),
            sort,
            key_format: options.value("-K").map(str::to_owned),
            template: chooser_command_template(invocation, positional_start, &positional),
            zoom: options.has("-Z"),
        }))
    }

    pub(super) fn display_panes(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("display-panes").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("display-panes", args)?;
        let template = positional
            .first()
            .map(|template| {
                if invocation.argument_is_command_block(positional_start) {
                    let body = crate::parser::command_block_body(template).unwrap_or(template);
                    self.prepare_frozen_callback_commands(body, true, "display-panes")
                        .map(CommandPromptTemplate::Commands)
                } else {
                    Ok(CommandPromptTemplate::String(template.to_string()))
                }
            })
            .transpose()?;
        let source_window = options
            .value("-s")
            .map(|target| self.resolve_window(Some(target), context.session, context.window))
            .transpose()?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let target_window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let source_window = source_window.unwrap_or(target_window);
        let source_session = if source_window == target_window {
            context
                .session
                .filter(|session| {
                    self.state
                        .sessions
                        .get(session)
                        .is_some_and(|state| state.windows.contains(&target_window))
                })
                .unwrap_or(self.state.windows[&target_window].session)
        } else {
            self.state.windows[&source_window].session
        };
        Ok(Execution::effect(MuxEffect::DisplayPanes {
            pane,
            delay: options.value("-d").map(str::to_owned),
            selectable: !options.has("-N"),
            template,
            source: invocation.source.clone(),
            source_window,
            source_session,
            kill_source: options.has("-k"),
            zoom: !options.has("-Z"),
        }))
    }
}

fn chooser_preview(options: &Options) -> ChooserPreviewSize {
    match options.count("-N") {
        0 => ChooserPreviewSize::Normal,
        1 => ChooserPreviewSize::Off,
        _ => ChooserPreviewSize::Big,
    }
}

fn chooser_command_template(
    invocation: &CommandInvocation,
    positional_start: usize,
    positional: &[RawText],
) -> Option<String> {
    positional.first().map(|template| {
        if invocation.argument_is_command_block(positional_start) {
            crate::parser::command_block_body(template)
                .unwrap_or(template)
                .trim()
                .replace('\n', " ;; ")
        } else {
            template.to_string()
        }
    })
}
