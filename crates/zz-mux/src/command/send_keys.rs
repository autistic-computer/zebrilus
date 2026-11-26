use super::*;

impl MuxEngine {
    pub(super) fn send_keys(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("send-keys", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let target_client = options.value("-c").map(str::to_owned);
        let repeat = if let Some(value) = options.value("-N") {
            let target = ExecutionContext::for_pane(&self.state, pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            let expanded = self.expand_pane_format(
                value,
                &target,
                context.session,
                context.target_format_client(),
                hooks,
            );
            parse_repeat_count(&expanded)?
        } else {
            1
        };
        if options.value("-N").is_some() && positional.is_empty() && !options.has("-X") {
            let mut execution = Execution::effect(MuxEffect::CopyModeRepeat {
                pane,
                count: repeat,
                target_client: target_client.clone(),
                require_mode: false,
            });
            if options.has("-R") {
                execution.effects.push(MuxEffect::ResetPane { pane });
            }
            return Ok(execution);
        }
        if options.has("-X") {
            let Some(command) = positional.first() else {
                return Ok(Execution::effect(MuxEffect::CopyModeRepeat {
                    pane,
                    count: if options.value("-N").is_some() {
                        repeat
                    } else {
                        0
                    },
                    target_client,
                    require_mode: true,
                }));
            };
            let mut arguments = positional[1..].to_vec();
            if options.has("-F")
                && SEND_KEYS_FORMAT_EXPANDED_ACTIONS.contains(&command.as_str())
                && let Some(index) = send_keys_search_string_index(&arguments)
                && let Some(argument) = arguments.get_mut(index)
            {
                let target = ExecutionContext::for_pane(&self.state, pane)
                    .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                *argument = self
                    .expand_pane_format(
                        argument,
                        &target,
                        context.session,
                        context.target_format_client(),
                        hooks,
                    )
                    .into();
                if argument.is_empty() {
                    return Ok(Execution::effect(MuxEffect::CopyModeRepeat {
                        pane,
                        count: 1,
                        target_client,
                        require_mode: true,
                    }));
                }
            }
            let Some(action) =
                copy_mode_action(command, &arguments, self.set_clipboard, &self.copy_command)?
            else {
                return Ok(Execution::effect(MuxEffect::CopyModeRepeat {
                    pane,
                    count: 1,
                    target_client,
                    require_mode: true,
                }));
            };
            let action = match action {
                CopyModeAction::LineNumbersOn { .. }
                | CopyModeAction::LineNumbersToggle { .. }
                | CopyModeAction::GotoLine { .. } => {
                    let target = ExecutionContext::for_pane(&self.state, pane)
                        .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                    let option = self.expand_pane_format(
                        "#{copy-mode-line-numbers}",
                        &target,
                        context.session,
                        context.target_format_client(),
                        hooks,
                    );
                    let option_off = option == "off";
                    match action {
                        CopyModeAction::LineNumbersOn { .. } => {
                            CopyModeAction::LineNumbersOn { option_off }
                        }
                        CopyModeAction::GotoLine { line, .. } => CopyModeAction::GotoLine {
                            line,
                            option_absolute: matches!(
                                option.as_str(),
                                "absolute" | "relative" | "hybrid"
                            ),
                        },
                        _ => CopyModeAction::LineNumbersToggle { option_off },
                    }
                }
                action => action,
            };
            let action = match action.count_policy() {
                CopyModeCountPolicy::Once => TerminalViewAction::CopyMode(action),
                _ if repeat == 1 => TerminalViewAction::CopyMode(action),
                _ => TerminalViewAction::CopyModeCounted {
                    action,
                    count: repeat,
                },
            };
            let mut effects = Self::mouse_cursor_move(context, pane);
            effects.push(MuxEffect::TerminalView {
                pane,
                action,
                target_client,
                require_mode: true,
            });
            return Ok(Execution {
                output: RawText::default(),
                effects,
            });
        }
        if options.has("-M") {
            let Some(mouse) = context.invoking_mouse() else {
                return Err(ServerError::InvalidCommand("no mouse target".to_owned()));
            };
            let Some(target) = mouse.pane else {
                return Err(ServerError::InvalidCommand("no mouse target".to_owned()));
            };
            let Some(action) = mouse.view_action.clone() else {
                return Ok(Execution::default());
            };
            return Ok(Execution::effect(MuxEffect::TerminalView {
                pane: target,
                action,
                target_client,
                require_mode: false,
            }));
        }
        let reset = options.has("-R");
        if reset && positional.is_empty() {
            return Ok(Execution::effect(MuxEffect::ResetPane { pane }));
        }
        if options.has("-K") {
            let mut execution = Execution::default();
            if reset {
                execution.effects.push(MuxEffect::ResetPane { pane });
            }
            let keys = if positional.is_empty() {
                context
                    .invoking_key()
                    .map(|key| vec![KeyToken::Named(key.to_owned())])
                    .unwrap_or_default()
            } else if options.has("-H") {
                positional
                    .iter()
                    .filter_map(|value| hex_key_token(value))
                    .collect()
            } else if options.has("-l") {
                vec![KeyToken::Literal(positional.concat())]
            } else {
                positional.iter().map(|value| key_token(value)).collect()
            };
            if !keys.is_empty() {
                execution.effects.push(MuxEffect::SendClientKeys {
                    target_client,
                    keys,
                    repeat,
                });
            }
            return Ok(execution);
        }
        let keys = if options.has("-H") {
            positional
                .iter()
                .filter_map(|value| hex_key_token(value))
                .collect()
        } else if options.has("-l") {
            vec![KeyToken::Literal(positional.concat())]
        } else {
            positional
                .iter()
                .filter_map(|value| {
                    if value == "BSpace" {
                        return match self.server_options.backspace.as_str() {
                            "\x7f" | "0x7f" => Some(KeyToken::Literal("\x7f".to_owned())),
                            "C-\x7f" => None,
                            _ => Some(key_token(value)),
                        };
                    }
                    Some(key_token(value))
                })
                .collect()
        };
        if keys.is_empty()
            && (options.has("-H")
                || (positional.iter().any(|value| value == "BSpace")
                    && self.server_options.backspace == "C-\x7f"))
        {
            return Ok(if reset {
                Execution::effect(MuxEffect::ResetPane { pane })
            } else {
                Execution::default()
            });
        }
        let mut execution = Execution::default();
        if reset {
            execution.effects.push(MuxEffect::ResetPane { pane });
        }
        execution
            .effects
            .push(MuxEffect::SendKeys { pane, keys, repeat });
        Ok(execution)
    }

    fn mouse_cursor_move(context: &ExecutionContext, pane: PaneId) -> Vec<MuxEffect> {
        let Some(mouse) = context.invoking_mouse() else {
            return Vec::new();
        };
        if mouse.pane != Some(pane) {
            return Vec::new();
        }
        match mouse.view_action.as_ref() {
            Some(TerminalViewAction::Mouse(input)) => vec![MuxEffect::TerminalView {
                pane,
                action: TerminalViewAction::CopyMode(CopyModeAction::MouseCursor(input.cell)),
                target_client: None,
                require_mode: true,
            }],
            _ => Vec::new(),
        }
    }

    pub(super) fn send_prefix(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, _) = parse_command_options("send-prefix", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let key = if options.has("-2") {
            let Some(prefix2) = self.keys.prefix2() else {
                return Ok(Execution::default());
            };
            prefix2
        } else {
            self.keys.prefix()
        };
        let key = match key {
            "\x7f" | "0x7f" => KeyToken::Literal("\x7f".to_owned()),
            "C-\x7f" => return Ok(Execution::default()),
            key => KeyToken::Named(tmux_key_display(key)),
        };
        Ok(Execution::effect(MuxEffect::SendKeys {
            pane,
            keys: vec![key],
            repeat: 1,
        }))
    }
}

fn key_token(value: &str) -> KeyToken {
    if let Some(name) = canonical_named_key(value) {
        return KeyToken::Named(name.to_owned());
    }
    const NAMED: &[&str] = &[
        "Enter", "Escape", "Space", "Tab", "BSpace", "Up", "Down", "Left", "Right", "Home", "End",
        "PPage", "NPage", "DC", "IC",
    ];
    if NAMED.contains(&value)
        || value.starts_with("C-")
        || value.starts_with("M-")
        || value
            .strip_prefix('F')
            .is_some_and(|number| number.parse::<u8>().is_ok())
    {
        KeyToken::Named(value.to_owned())
    } else {
        KeyToken::Literal(value.to_owned())
    }
}

fn hex_key_token(value: &str) -> Option<KeyToken> {
    let digits = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let code = u32::from_str_radix(digits, 16).ok()?;
    match code {
        0x00..=0x7f => Some(KeyToken::Literal(
            char::from_u32(code).expect("ascii range").to_string(),
        )),
        0x80..=0xff => Some(KeyToken::Raw(u8::try_from(code).expect("byte range"))),
        _ => None,
    }
}

fn parse_repeat_count(value: &str) -> Result<u32, ServerError> {
    u32::try_from(parse_strtonum(
        value,
        1,
        i64::from(u32::MAX),
        "repeat count",
    )?)
    .map_err(|_| ServerError::InvalidCommand("repeat count too large".to_owned()))
}

#[must_use]
pub fn copy_mode_action_is_read_only_safe(action: &CopyModeAction) -> bool {
    matches!(
        action,
        CopyModeAction::Left
            | CopyModeAction::Right
            | CopyModeAction::Up
            | CopyModeAction::Down
            | CopyModeAction::PageUp
            | CopyModeAction::PageDown
            | CopyModeAction::HalfPageUp
            | CopyModeAction::HalfPageDown
            | CopyModeAction::Top
            | CopyModeAction::Bottom
            | CopyModeAction::TopLine
            | CopyModeAction::MiddleLine
            | CopyModeAction::BottomLine
            | CopyModeAction::StartOfLine
            | CopyModeAction::BackToIndentation
            | CopyModeAction::EndOfLine
            | CopyModeAction::NextWord
            | CopyModeAction::PreviousWord
            | CopyModeAction::NextWordEnd
            | CopyModeAction::NextParagraph
            | CopyModeAction::PreviousParagraph
            | CopyModeAction::NextPrompt { .. }
            | CopyModeAction::PreviousPrompt { .. }
            | CopyModeAction::SetMark
            | CopyModeAction::JumpToMark
            | CopyModeAction::MouseCursor(_)
            | CopyModeAction::Cancel
            | CopyModeAction::NextSpace
            | CopyModeAction::PreviousSpace
            | CopyModeAction::NextSpaceEnd
            | CopyModeAction::ScrollUp
            | CopyModeAction::ScrollDown
            | CopyModeAction::ScrollMiddle
            | CopyModeAction::ScrollTop
            | CopyModeAction::ScrollBottom
            | CopyModeAction::CursorCentreVertical
            | CopyModeAction::CursorCentreHorizontal
            | CopyModeAction::TogglePosition
            | CopyModeAction::RefreshOn
            | CopyModeAction::RefreshOff
            | CopyModeAction::RefreshToggle
            | CopyModeAction::RefreshNow
            | CopyModeAction::LineNumbersOn { .. }
            | CopyModeAction::LineNumbersOff
            | CopyModeAction::LineNumbersToggle { .. }
            | CopyModeAction::RefreshRevision
            | CopyModeAction::RecentreTopBottom
            | CopyModeAction::NextMatchingBracket
            | CopyModeAction::PreviousMatchingBracket
            | CopyModeAction::GotoLine { .. }
            | CopyModeAction::PageDownScrollExit
            | CopyModeAction::HalfPageDownScrollExit
            | CopyModeAction::ScrollDownAndCancel
            | CopyModeAction::CursorDownAndCancel
    )
}

pub fn send_keys_is_read_only_safe(args: &[RawText]) -> Result<bool, ServerError> {
    let spec = command_spec("send-keys").expect("send-keys has catalog metadata");
    let (unvalidated_options, _) = parse_options_for_spec(args, spec)?;
    if !unvalidated_options.has("-X") {
        return Ok(false);
    }
    let (_, positional) = parse_command_options("send-keys", args)?;
    let Some(command) = positional.first() else {
        return Ok(true);
    };
    let probe = copy_mode_probe_action(command);
    if probe.is_none()
        && let Some(pinned) = pinned_copy_action(command)
    {
        return Ok(pinned.read_only);
    }
    if let Ok(Some(action)) = copy_mode_action(command, &positional[1..], SetClipboard::Off, "") {
        return Ok(copy_mode_action_is_read_only_safe(&action));
    }
    Ok(probe
        .as_ref()
        .is_none_or(copy_mode_action_is_read_only_safe))
}

pub fn send_keys_target_client(args: &[RawText]) -> Result<Option<String>, ServerError> {
    let spec = command_spec("send-keys").expect("send-keys has catalog metadata");
    let (options, _) = parse_options_for_spec(args, spec)?;
    Ok(options.value("-c").map(str::to_owned))
}

pub(crate) fn copy_mode_probe_action(command: &str) -> Option<CopyModeAction> {
    let arguments: Vec<RawText> = match command {
        "goto-line" => vec!["1".into()],
        "jump-forward" | "jump-backward" | "jump-to-forward" | "jump-to-backward" => {
            vec!["x".into()]
        }
        "search-forward-incremental" | "search-backward-incremental" => vec!["=x".into()],
        _ => Vec::new(),
    };
    copy_mode_action(command, &arguments, SetClipboard::Off, "")
        .ok()
        .flatten()
}

fn copy_mode_action(
    command: &str,
    arguments: &[RawText],
    set_clipboard: SetClipboard,
    copy_command: &str,
) -> Result<Option<CopyModeAction>, ServerError> {
    let Some((options, arguments)) = copy_mode_action_options(command, arguments) else {
        return Ok(None);
    };
    let output = options.has("-o");
    let action = match command {
        "cursor-left" => Some(CopyModeAction::Left),
        "cursor-right" => Some(CopyModeAction::Right),
        "cursor-up" => Some(CopyModeAction::Up),
        "cursor-down" => Some(CopyModeAction::Down),
        "page-up" => Some(CopyModeAction::PageUp),
        "page-down" => Some(CopyModeAction::PageDown),
        "page-down-and-cancel" => Some(CopyModeAction::PageDownScrollExit),
        "halfpage-up" => Some(CopyModeAction::HalfPageUp),
        "halfpage-down" => Some(CopyModeAction::HalfPageDown),
        "halfpage-down-and-cancel" => Some(CopyModeAction::HalfPageDownScrollExit),
        "cursor-down-and-cancel" => Some(CopyModeAction::CursorDownAndCancel),
        "history-top" => Some(CopyModeAction::Top),
        "history-bottom" => Some(CopyModeAction::Bottom),
        "top-line" => Some(CopyModeAction::TopLine),
        "middle-line" => Some(CopyModeAction::MiddleLine),
        "bottom-line" => Some(CopyModeAction::BottomLine),
        "start-of-line" => Some(CopyModeAction::StartOfLine),
        "back-to-indentation" => Some(CopyModeAction::BackToIndentation),
        "end-of-line" => Some(CopyModeAction::EndOfLine),
        "next-word" => Some(CopyModeAction::NextWord),
        "previous-word" => Some(CopyModeAction::PreviousWord),
        "next-word-end" => Some(CopyModeAction::NextWordEnd),
        "next-space" => Some(CopyModeAction::NextSpace),
        "previous-space" => Some(CopyModeAction::PreviousSpace),
        "next-space-end" => Some(CopyModeAction::NextSpaceEnd),
        "scroll-up" => Some(CopyModeAction::ScrollUp),
        "scroll-down" => Some(CopyModeAction::ScrollDown),
        "scroll-down-and-cancel" => Some(CopyModeAction::ScrollDownAndCancel),
        "scroll-middle" => Some(CopyModeAction::ScrollMiddle),
        "scroll-top" => Some(CopyModeAction::ScrollTop),
        "scroll-bottom" => Some(CopyModeAction::ScrollBottom),
        "cursor-centre-vertical" => Some(CopyModeAction::CursorCentreVertical),
        "cursor-centre-horizontal" => Some(CopyModeAction::CursorCentreHorizontal),
        "toggle-position" => Some(CopyModeAction::TogglePosition),
        "refresh-on" => Some(CopyModeAction::RefreshOn),
        "refresh-off" => Some(CopyModeAction::RefreshOff),
        "refresh-toggle" => Some(CopyModeAction::RefreshToggle),
        "refresh-now" => Some(CopyModeAction::RefreshNow),
        "line-numbers-on" => Some(CopyModeAction::LineNumbersOn { option_off: true }),
        "line-numbers-off" => Some(CopyModeAction::LineNumbersOff),
        "line-numbers-toggle" => Some(CopyModeAction::LineNumbersToggle { option_off: true }),
        "recentre-top-bottom" => Some(CopyModeAction::RecentreTopBottom),
        "scroll-exit-on" => Some(CopyModeAction::ScrollExitOn),
        "scroll-exit-off" => Some(CopyModeAction::ScrollExitOff),
        "scroll-exit-toggle" => Some(CopyModeAction::ScrollExitToggle),
        "goto-line" => copy_goto_line_action(&arguments),
        "next-matching-bracket" => Some(CopyModeAction::NextMatchingBracket),
        "previous-matching-bracket" => Some(CopyModeAction::PreviousMatchingBracket),
        "search-forward-cursor-word" => Some(CopyModeAction::SearchCursorWord {
            direction: SearchDirection::Forward,
        }),
        "search-backward-cursor-word" => Some(CopyModeAction::SearchCursorWord {
            direction: SearchDirection::Backward,
        }),
        "next-paragraph" => Some(CopyModeAction::NextParagraph),
        "previous-paragraph" => Some(CopyModeAction::PreviousParagraph),
        "next-prompt" => Some(CopyModeAction::NextPrompt { output }),
        "previous-prompt" => Some(CopyModeAction::PreviousPrompt { output }),
        "search-again" => Some(CopyModeAction::SearchAgain { reverse: false }),
        "search-reverse" => Some(CopyModeAction::SearchAgain { reverse: true }),
        "search-forward" | "search-backward" | "search-forward-text" | "search-backward-text" => {
            Some(copy_search_action(command, arguments.first()))
        }
        "search-forward-incremental" | "search-backward-incremental" => {
            copy_incremental_search_action(command, arguments.first())
        }
        "begin-selection" => Some(CopyModeAction::StartSelection),
        "select-word" => Some(CopyModeAction::SelectWord),
        "select-line" => Some(CopyModeAction::SelectLine),
        "selection-mode" => copy_selection_mode(arguments.first().map(RawText::as_str))
            .map(CopyModeAction::SelectionMode),
        "clear-selection" => Some(CopyModeAction::ClearSelection),
        "stop-selection" => Some(CopyModeAction::StopSelection),
        "clear-selection-or-cancel" => Some(CopyModeAction::ClearSelectionOrCancel),
        "rectangle-toggle" => Some(CopyModeAction::ToggleRectangle),
        "rectangle-on" => Some(CopyModeAction::RectangleOn),
        "rectangle-off" => Some(CopyModeAction::RectangleOff),
        "other-end" => Some(CopyModeAction::OtherEnd),
        "set-mark" => Some(CopyModeAction::SetMark),
        "jump-to-mark" => Some(CopyModeAction::JumpToMark),
        "jump-forward" => copy_jump_action(&arguments, CopyJumpDirection::Forward, false),
        "jump-backward" => copy_jump_action(&arguments, CopyJumpDirection::Backward, false),
        "jump-to-forward" => copy_jump_action(&arguments, CopyJumpDirection::Forward, true),
        "jump-to-backward" => copy_jump_action(&arguments, CopyJumpDirection::Backward, true),
        "jump-again" => Some(CopyModeAction::RepeatJump { reverse: false }),
        "jump-reverse" => Some(CopyModeAction::RepeatJump { reverse: true }),
        "copy-selection" => Some(copy_selection_action(
            &arguments,
            &options,
            set_clipboard,
            true,
            false,
        )),
        "copy-selection-no-clear" => Some(copy_selection_action(
            &arguments,
            &options,
            set_clipboard,
            false,
            false,
        )),
        "copy-selection-and-cancel" => Some(copy_selection_action(
            &arguments,
            &options,
            set_clipboard,
            true,
            true,
        )),
        "append-selection" | "append-selection-and-cancel" => {
            Some(CopyModeAction::copy_selection(CopyModeCopy {
                request_id: 0,
                clipboard: false,
                buffer: Some(PasteBufferAction::Append),
                pipe: None,
                clear_selection: true,
                cancel: command.ends_with("-and-cancel"),
            }))
        }
        "copy-pipe" | "copy-pipe-no-clear" | "copy-pipe-and-cancel" => Some(copy_pipe_action(
            command,
            &arguments,
            &options,
            set_clipboard,
            copy_command,
            true,
        )?),
        "pipe" | "pipe-no-clear" | "pipe-and-cancel" => Some(copy_pipe_action(
            command,
            &arguments,
            &options,
            set_clipboard,
            copy_command,
            false,
        )?),
        "copy-pipe-end-of-line" | "copy-pipe-end-of-line-and-cancel" => {
            Some(copy_end_of_line_action(copy_pipe_action(
                command,
                &arguments,
                &options,
                set_clipboard,
                copy_command,
                true,
            )?))
        }
        "copy-pipe-line" | "copy-pipe-line-and-cancel" => Some(copy_line_action(copy_pipe_action(
            command,
            &arguments,
            &options,
            set_clipboard,
            copy_command,
            true,
        )?)),
        "copy-line" | "copy-line-and-cancel" => Some(copy_line_action(copy_selection_action(
            &arguments,
            &options,
            set_clipboard,
            true,
            command.ends_with("-and-cancel"),
        ))),
        "copy-end-of-line" | "copy-end-of-line-and-cancel" => {
            Some(copy_end_of_line_action(copy_selection_action(
                &arguments,
                &options,
                set_clipboard,
                true,
                command.ends_with("-and-cancel"),
            )))
        }
        "cancel" => Some(CopyModeAction::Cancel),
        _ => None,
    };
    Ok(action)
}

fn send_keys_search_string_index(arguments: &[RawText]) -> Option<usize> {
    let mut index = 0;
    while let Some(argument) = arguments.get(index) {
        if argument == "--" {
            return Some(index + 1);
        }
        if argument.starts_with('-') && argument != "-" {
            index += 1;
            continue;
        }
        return Some(index);
    }
    None
}

const SEND_KEYS_FORMAT_EXPANDED_ACTIONS: &[&str] = &[
    "search-backward",
    "search-backward-text",
    "search-forward",
    "search-forward-text",
];

fn copy_search_action(command: &str, argument: Option<&RawText>) -> CopyModeAction {
    CopyModeAction::Search(Box::new(CopyModeSearch {
        text: argument.map(ToString::to_string).unwrap_or_default(),
        direction: if command.starts_with("search-forward") {
            SearchDirection::Forward
        } else {
            SearchDirection::Backward
        },
        regex: !command.ends_with("-text"),
        incremental: false,
    }))
}

fn copy_incremental_search_action(
    command: &str,
    argument: Option<&RawText>,
) -> Option<CopyModeAction> {
    let argument = argument?;
    let mut characters = argument.chars();
    let prefix = characters.next()?;
    let forward_spelling = command.starts_with("search-forward");
    let direction = match prefix {
        '=' => {
            if forward_spelling {
                SearchDirection::Forward
            } else {
                SearchDirection::Backward
            }
        }
        '+' => SearchDirection::Forward,
        '-' => SearchDirection::Backward,
        _ => return None,
    };
    Some(CopyModeAction::Search(Box::new(CopyModeSearch {
        text: characters.as_str().to_owned(),
        direction,
        regex: false,
        incremental: true,
    })))
}

fn copy_mode_action_options(
    command: &str,
    arguments: &[RawText],
) -> Option<(Options, Vec<RawText>)> {
    let (options, positional) = parse_options(arguments, &[], &[]).ok()?;
    let allowed = match command {
        "copy-selection"
        | "copy-selection-no-clear"
        | "copy-selection-and-cancel"
        | "copy-pipe"
        | "copy-pipe-no-clear"
        | "copy-pipe-and-cancel"
        | "copy-pipe-end-of-line"
        | "copy-pipe-end-of-line-and-cancel"
        | "copy-end-of-line"
        | "copy-end-of-line-and-cancel"
        | "copy-line"
        | "copy-line-and-cancel"
        | "copy-pipe-line"
        | "copy-pipe-line-and-cancel" => &["-C", "-P"][..],
        "next-prompt" | "previous-prompt" => &["-o"][..],
        _ => &[],
    };
    if options
        .flags
        .iter()
        .any(|flag| !allowed.contains(&flag.as_str()))
    {
        return None;
    }
    let (minimum, maximum) = match command {
        "goto-line"
        | "jump-forward"
        | "jump-backward"
        | "jump-to-forward"
        | "jump-to-backward"
        | "search-forward-incremental"
        | "search-backward-incremental" => (1, 1),
        "copy-selection"
        | "copy-selection-no-clear"
        | "copy-selection-and-cancel"
        | "copy-end-of-line"
        | "copy-end-of-line-and-cancel"
        | "copy-line"
        | "copy-line-and-cancel"
        | "pipe"
        | "pipe-no-clear"
        | "pipe-and-cancel"
        | "search-forward"
        | "search-backward"
        | "search-forward-text"
        | "search-backward-text"
        | "selection-mode" => (0, 1),
        "copy-pipe"
        | "copy-pipe-no-clear"
        | "copy-pipe-and-cancel"
        | "copy-pipe-end-of-line"
        | "copy-pipe-end-of-line-and-cancel"
        | "copy-pipe-line"
        | "copy-pipe-line-and-cancel" => (0, 2),
        _ => (0, 0),
    };
    if positional.len() < minimum || positional.len() > maximum {
        return None;
    }
    Some((options, positional))
}

fn copy_end_of_line_action(action: CopyModeAction) -> CopyModeAction {
    let CopyModeAction::CopySelection(copy) = action else {
        unreachable!("copy helper always returns a selection action");
    };
    CopyModeAction::CopyEndOfLine(copy)
}

fn copy_line_action(action: CopyModeAction) -> CopyModeAction {
    let CopyModeAction::CopySelection(copy) = action else {
        unreachable!("copy helper always returns a selection action");
    };
    CopyModeAction::CopyLine(copy)
}

fn copy_selection_mode(argument: Option<&str>) -> Option<CopySelectionMode> {
    match argument {
        None => Some(CopySelectionMode::Char),
        Some(value) if value.eq_ignore_ascii_case("char") || value.eq_ignore_ascii_case("c") => {
            Some(CopySelectionMode::Char)
        }
        Some(value) if value.eq_ignore_ascii_case("word") || value.eq_ignore_ascii_case("w") => {
            Some(CopySelectionMode::Word)
        }
        Some(value) if value.eq_ignore_ascii_case("line") || value.eq_ignore_ascii_case("l") => {
            Some(CopySelectionMode::Line)
        }
        Some(_) => None,
    }
}

fn copy_goto_line_action(arguments: &[RawText]) -> Option<CopyModeAction> {
    let [line] = arguments else { return None };
    let line = pinned_strtonum(line, -1, i64::from(i32::MAX))
        .and_then(|parsed| i32::try_from(parsed).ok());
    Some(CopyModeAction::GotoLine {
        line,
        option_absolute: false,
    })
}

fn pinned_strtonum(value: &str, minimum: i64, maximum: i64) -> Option<i64> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == 0x0b)
    {
        index += 1;
    }
    let start = index;
    if matches!(bytes.get(index), Some(b'+' | b'-')) {
        index += 1;
    }
    let digits = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == digits || index != bytes.len() {
        return None;
    }
    let parsed = value.get(start..index)?.parse::<i64>().ok()?;
    (minimum..=maximum).contains(&parsed).then_some(parsed)
}

fn copy_pipe_action(
    command_name: &str,
    arguments: &[RawText],
    options: &Options,
    set_clipboard: SetClipboard,
    copy_command: &str,
    copy: bool,
) -> Result<CopyModeAction, ServerError> {
    let pipe = arguments.first().map_or(copy_command, RawText::as_str);
    if pipe.len() > MAX_COPY_COMMAND_BYTES {
        return Err(ServerError::InvalidCommand(format!(
            "copy pipe command exceeds {MAX_COPY_COMMAND_BYTES} bytes"
        )));
    }
    let prefix = if copy {
        arguments.get(1).map(ToString::to_string)
    } else {
        None
    };
    Ok(CopyModeAction::copy_selection(CopyModeCopy {
        request_id: 0,
        clipboard: copy && !options.has("-C") && set_clipboard != SetClipboard::Off,
        buffer: (copy && !options.has("-P")).then_some(PasteBufferAction::Create { prefix }),
        pipe: (!pipe.is_empty()).then(|| pipe.to_owned()),
        clear_selection: !command_name.ends_with("-no-clear"),
        cancel: command_name.ends_with("-and-cancel"),
    }))
}

fn copy_jump_action(
    arguments: &[RawText],
    direction: CopyJumpDirection,
    to: bool,
) -> Option<CopyModeAction> {
    if arguments.len() != 1 || arguments[0].is_empty() {
        return None;
    }
    Some(CopyModeAction::Jump(CopyJump {
        target: arguments[0].to_string(),
        direction,
        to,
    }))
}

fn copy_selection_action(
    arguments: &[RawText],
    options: &Options,
    set_clipboard: SetClipboard,
    clear_selection: bool,
    cancel: bool,
) -> CopyModeAction {
    CopyModeAction::copy_selection(CopyModeCopy {
        request_id: 0,
        clipboard: !options.has("-C") && set_clipboard != SetClipboard::Off,
        buffer: (!options.has("-P")).then(|| PasteBufferAction::Create {
            prefix: arguments.first().map(ToString::to_string),
        }),
        pipe: None,
        clear_selection,
        cancel,
    })
}
