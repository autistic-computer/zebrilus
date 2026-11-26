use super::*;

impl MuxEngine {
    #[must_use]
    pub fn evaluate_config_condition(&self, condition: &str) -> bool {
        let mut hooks = ConfigConditionHooks {
            engine: self,
            inner: CommandHooks::new(self.format_now()),
        };
        let expanded =
            expand_format_with_hooks(condition, self, FormatContext::default(), &mut hooks);
        format_true(&expanded)
    }

    #[must_use]
    pub fn parse_config(&self, source: impl Into<String>, input: &str) -> crate::ParsedConfig {
        let mut context = (
            |name: &str| self.global_environment_bytes(name),
            |condition: &str| self.evaluate_config_condition(condition),
        );
        crate::parser::parse_config_with(source, input, &mut context)
    }

    #[must_use]
    pub fn parse_config_raw(
        &self,
        source: impl Into<String>,
        input: &RawText,
    ) -> crate::ParsedConfig {
        match input.to_utf8() {
            Some(text) => self.parse_config(source, text),
            None => self
                .parse_config_file_bytes(source, input.as_bytes())
                .into_invocations(),
        }
    }

    #[must_use]
    pub fn parse_config_parse_only(
        &self,
        source: impl Into<String>,
        input: &str,
    ) -> crate::ParsedConfig {
        let mut context = (
            |name: &str| self.global_environment_bytes(name),
            |condition: &str| self.evaluate_config_condition(condition),
        );
        crate::parser::parse_config_without_assignment_overlay(source, input, &mut context)
    }

    #[must_use]
    pub fn parse_config_file_bytes(
        &self,
        source: impl Into<String>,
        input: &[u8],
    ) -> crate::ParsedConfigBytes {
        let mut context = (
            |name: &str| self.global_environment_bytes(name),
            |condition: &str| self.evaluate_config_condition(condition),
        );
        crate::parser::parse_config_file_bytes_with_assignment_overlay(
            source,
            input,
            &mut context,
            true,
        )
    }

    #[must_use]
    pub fn parse_config_file_bytes_parse_only(
        &self,
        source: impl Into<String>,
        input: &[u8],
    ) -> crate::ParsedConfigBytes {
        let mut context = (
            |name: &str| self.global_environment_bytes(name),
            |condition: &str| self.evaluate_config_condition(condition),
        );
        crate::parser::parse_config_file_bytes_with_assignment_overlay(
            source,
            input,
            &mut context,
            false,
        )
    }

    #[must_use]
    pub fn parse_config_buffer_bytes(
        &self,
        source: impl Into<String>,
        input: &[u8],
    ) -> crate::ParsedConfigBytes {
        let mut context = (
            |name: &str| self.global_environment_bytes(name),
            |condition: &str| self.evaluate_config_condition(condition),
        );
        crate::parser::parse_config_buffer_bytes_with_assignment_overlay(
            source,
            input,
            &mut context,
            true,
        )
    }

    pub fn prepare_callback_commands(
        &self,
        input: &str,
        source_groups: bool,
        owner: &str,
    ) -> Result<Vec<CommandInvocation>, ServerError> {
        prepare_callback_commands_with_aliases(
            self,
            &RawText::from(input),
            source_groups,
            owner,
            true,
            true,
        )
    }

    pub fn prepare_frozen_callback_commands(
        &self,
        input: &str,
        source_groups: bool,
        owner: &str,
    ) -> Result<Vec<CommandInvocation>, ServerError> {
        prepare_callback_commands_with_aliases(
            self,
            &RawText::from(input),
            source_groups,
            owner,
            false,
            false,
        )
    }

    pub fn prepare_frozen_callback_invocations(
        &self,
        commands: &mut [CommandInvocation],
        owner: &str,
    ) -> Result<(), ServerError> {
        prepare_callback_invocations_with_aliases(self, commands, owner, false, false)
    }

    pub fn prepare_callback_invocations(
        &self,
        commands: &mut [CommandInvocation],
        owner: &str,
    ) -> Result<(), ServerError> {
        prepare_callback_invocations(self, commands, owner)
    }

    pub fn prepare_expanded_callback_command(
        &self,
        command: &mut CommandInvocation,
        owner: &str,
        alias_matched: bool,
    ) -> Result<(), ServerError> {
        prepare_expanded_callback_invocation(self, command, owner, !alias_matched)
    }

    pub fn substitute_command_prompt_commands<S: AsRef<str>>(
        &self,
        commands: &mut [CommandInvocation],
        inputs: &[S],
    ) -> Result<(), ServerError> {
        substitute_command_prompt_invocations(commands, inputs)
    }

    #[must_use]
    pub fn substitute_command_prompt_template<S: AsRef<str>>(
        template: &str,
        inputs: &[S],
    ) -> String {
        inputs
            .iter()
            .enumerate()
            .fold(template.to_owned(), |template, (index, input)| {
                let index = u8::try_from(index.saturating_add(1)).unwrap_or(u8::MAX);
                command_template_replace(&template, input.as_ref(), index)
            })
    }

    pub fn parse_config_without_variable_expansion(
        source: impl Into<String>,
        input: &str,
    ) -> crate::ParsedConfig {
        crate::parser::parse_config_without_variable_expansion(source, input)
    }
}

#[must_use]
pub fn if_shell_truthy(value: &str) -> bool {
    value.as_bytes().first().is_some_and(|first| *first != b'0')
}

pub(super) fn bound_commands(
    engine: &MuxEngine,
    owner: &CommandInvocation,
    start: usize,
) -> Result<Vec<CommandInvocation>, ServerError> {
    let tail = &owner.args[start..];
    if owner.argument_is_command_block(start) {
        if let [argument] = tail {
            return prepare_callback_commands_with_aliases(
                engine,
                &raw_command_block_body(argument),
                true,
                "bind-key",
                false,
                false,
            );
        }
        return Ok(Vec::new());
    }
    if let [argument] = tail {
        return engine
            .prepare_callback_commands(argument, false, "bind-key")
            .map_err(|error| callback_construction_error("bind-key", error));
    }
    let mut commands = owner.split_commands_from(start);
    let result =
        prepare_callback_invocations_with_aliases(engine, &mut commands, "bind-key", true, false);
    result.map_err(|error| callback_construction_error("bind-key", error))?;
    Ok(commands)
}

fn raw_command_block_body(argument: &RawText) -> RawText {
    argument
        .as_bytes()
        .strip_prefix(b"{")
        .and_then(|rest| rest.strip_suffix(b"}"))
        .map_or_else(|| argument.clone(), RawText::from_bytes)
}

fn parse_bound_command_string(
    engine: &MuxEngine,
    input: &RawText,
    source_groups: bool,
    owner: &str,
) -> Result<Vec<CommandInvocation>, ServerError> {
    let parsed = engine.parse_config_raw(format!("<{owner}>"), input);
    if let Some(diagnostic) = parsed.diagnostics.into_iter().next() {
        return Err(ServerError::CommandParse(diagnostic.message));
    }
    let mut commands = parsed.commands;
    retain_callback_source_groups(&mut commands, source_groups);
    Ok(commands)
}

fn prepare_callback_commands_with_aliases(
    engine: &MuxEngine,
    input: &RawText,
    source_groups: bool,
    owner: &str,
    command_aliases_available: bool,
    block_aliases_available: bool,
) -> Result<Vec<CommandInvocation>, ServerError> {
    let mut commands = parse_bound_command_string(engine, input, source_groups, owner)?;
    prepare_callback_invocations_with_aliases(
        engine,
        &mut commands,
        owner,
        command_aliases_available,
        block_aliases_available,
    )?;
    Ok(commands)
}

fn retain_callback_source_groups(commands: &mut [CommandInvocation], source_groups: bool) {
    let multiple_groups = source_groups
        && commands.windows(2).any(|commands| {
            commands[0]
                .source
                .as_ref()
                .map(|source| (&source.source, source.line))
                != commands[1]
                    .source
                    .as_ref()
                    .map(|source| (&source.source, source.line))
        });
    if !multiple_groups {
        for command in commands {
            command.source = None;
        }
    }
}

fn prepare_callback_invocations(
    engine: &MuxEngine,
    commands: &mut [CommandInvocation],
    owner: &str,
) -> Result<(), ServerError> {
    prepare_callback_invocations_with_aliases(engine, commands, owner, true, true)
}

fn prepare_callback_invocations_with_aliases(
    engine: &MuxEngine,
    commands: &mut [CommandInvocation],
    owner: &str,
    command_aliases_available: bool,
    block_aliases_available: bool,
) -> Result<(), ServerError> {
    for command in commands {
        prepare_callback_invocation(
            engine,
            command,
            owner,
            command_aliases_available,
            block_aliases_available,
        )?;
    }
    Ok(())
}

fn prepare_callback_invocation(
    engine: &MuxEngine,
    command: &mut CommandInvocation,
    owner: &str,
    command_aliases_available: bool,
    block_aliases_available: bool,
) -> Result<(), ServerError> {
    let alias_matched = expand_stored_alias(engine, command, command_aliases_available)?;
    prepare_expanded_callback_invocation(
        engine,
        command,
        owner,
        !alias_matched && block_aliases_available,
    )
}

fn prepare_expanded_callback_invocation(
    engine: &MuxEngine,
    command: &mut CommandInvocation,
    owner: &str,
    aliases_available: bool,
) -> Result<(), ServerError> {
    if let Some(body) = command_alias_group_body(command) {
        let commands = prepare_callback_commands_with_aliases(
            engine,
            &RawText::from(body),
            true,
            owner,
            false,
            false,
        )?;
        command.args[0] =
            format!("{{ {} }}", format_callback_commands_round_trip(&commands)).into();
        return validate_static_command_chain(&commands);
    }
    if canonical_command(&command.name) == "display-menu" {
        for index in 0..command.args.len() {
            if !command.argument_is_command_block(index) {
                continue;
            }
            let value = &command.args[index];
            let body = crate::parser::command_block_body(value).unwrap_or(value);
            let parsed =
                crate::parser::parse_config_without_variable_expansion("<menu-item>", body);
            if let Some(diagnostic) = parsed.diagnostics.into_iter().next() {
                return Err(ServerError::CommandParse(diagnostic.message));
            }
            validate_menu_item_commands(&parsed.commands)?;
        }
        return validate_bound_command(command, owner);
    }
    let menu_items = bind_key_menu_item_blocks(command);
    for index in 0..command.args.len() {
        if !command.argument_is_command_block(index) {
            continue;
        }
        let body = raw_command_block_body(&command.args[index]);
        let item_owner = if menu_items.contains(&index) {
            "<menu-item>"
        } else {
            owner
        };
        let commands = prepare_callback_commands_with_aliases(
            engine,
            &body,
            true,
            item_owner,
            aliases_available,
            aliases_available,
        )?;
        let body = format_callback_commands_round_trip(&commands);
        command.args[index] = format!("{{ {body} }}").into();
    }
    validate_bound_command(command, owner)
}

fn substitute_command_prompt_invocations<S: AsRef<str>>(
    commands: &mut [CommandInvocation],
    inputs: &[S],
) -> Result<(), ServerError> {
    for command in commands {
        for index in 0..command.args.len() {
            if !command.argument_is_command_block(index) {
                command.args[index] =
                    MuxEngine::substitute_command_prompt_template(&command.args[index], inputs)
                        .into();
                continue;
            }
            let body = crate::parser::command_block_body(&command.args[index])
                .unwrap_or(&command.args[index]);
            let parsed =
                crate::parser::parse_config_without_variable_expansion("<command-prompt>", body);
            if let Some(diagnostic) = parsed.diagnostics.into_iter().next() {
                return Err(ServerError::CommandParse(diagnostic.message));
            }
            let mut nested = parsed.commands;
            substitute_command_prompt_invocations(&mut nested, inputs)?;
            command.args[index] =
                format!("{{ {} }}", format_callback_commands_round_trip(&nested)).into();
        }
    }
    Ok(())
}

fn command_template_replace(template: &str, input: &str, index: u8) -> String {
    if !template.as_bytes().contains(&b'%') {
        return template.to_owned();
    }

    let bytes = template.as_bytes();
    let mut output = Vec::with_capacity(template.len().saturating_add(input.len()));
    let mut offset = 0;
    let mut replaced_double = false;
    while offset < bytes.len() {
        if bytes[offset] != b'%' {
            output.push(bytes[offset]);
            offset += 1;
            continue;
        }

        let next = bytes.get(offset + 1).copied();
        let indexed = (1..=9).contains(&index) && next == Some(b'0' + index);
        let doubled = next == Some(b'%') && !replaced_double;
        if !indexed && !doubled {
            output.push(bytes[offset]);
            offset += 1;
            continue;
        }
        replaced_double |= doubled;
        offset += 2;
        let double_quoted = bytes.get(offset) == Some(&b'%');
        if double_quoted {
            offset += 1;
        }
        let single_quoted = doubled && !double_quoted;
        for byte in input.bytes() {
            if single_quoted && byte == b'\'' {
                output.extend_from_slice(b"'\\''");
                continue;
            }
            if double_quoted && matches!(byte, b'"' | b'\\' | b'$' | b';' | b'~') {
                output.push(b'\\');
            }
            output.push(byte);
        }
    }
    String::from_utf8(output).expect("template replacement preserves UTF-8")
}

fn callback_construction_error(owner: &str, error: ServerError) -> ServerError {
    match (owner, error) {
        ("bind-key", ServerError::CommandParse(message)) => ServerError::InvalidCommand(message),
        ("bind-key", ServerError::NativeCommandParse(message)) => {
            ServerError::NativeInvalidCommand(message)
        }
        (_, error) => error,
    }
}

pub(super) fn parse_hook_commands(
    engine: &MuxEngine,
    value: &str,
) -> Result<Vec<CommandInvocation>, ServerError> {
    let input = value;
    if has_unquoted_hook_format(input) {
        return Err(ServerError::InvalidCommand("syntax error".to_owned()));
    }
    let parsed = engine.parse_config("<set-hook>", input);
    if let Some(diagnostic) = parsed.diagnostics.into_iter().next() {
        return Err(ServerError::InvalidCommand(diagnostic.message));
    }
    let mut commands = parsed.commands;
    prepare_callback_invocations(engine, &mut commands, "set-hook")?;
    Ok(commands)
}

pub(super) fn normalize_option_command(
    engine: &MuxEngine,
    value: &str,
) -> Result<String, ServerError> {
    normalize_option_command_with_groups(engine, value, false, true)
}

pub(super) fn normalize_typed_command_block(
    engine: &MuxEngine,
    value: &str,
) -> Result<String, ServerError> {
    normalize_option_command_with_groups(engine, value, true, false)
}

fn normalize_option_command_with_groups(
    engine: &MuxEngine,
    value: &str,
    source_groups: bool,
    aliases_available: bool,
) -> Result<String, ServerError> {
    let parsed = engine.parse_config("<set-option>", value);
    if !parsed.diagnostics.is_empty() {
        return Err(ServerError::InvalidCommand("syntax error".to_owned()));
    }
    let mut commands = parsed.commands;
    prepare_callback_invocations_with_aliases(
        engine,
        &mut commands,
        "set-option",
        aliases_available,
        aliases_available,
    )?;

    let mut output = String::new();
    let mut previous_line = None;
    for command in commands {
        let line = command.source.as_ref().map(|source| source.line);
        if !output.is_empty() {
            output.push_str(
                if source_groups
                    && previous_line
                        .zip(line)
                        .is_some_and(|(previous, current)| previous != current)
                {
                    " ;; "
                } else {
                    " ; "
                },
            );
        }
        output.push_str(&format_command(&command));
        previous_line = line;
    }
    Ok(output)
}

fn has_unquoted_hook_format(input: &str) -> bool {
    let mut characters = input.chars().peekable();
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;
    let mut word_started = false;
    while let Some(character) = characters.next() {
        if escaped {
            escaped = false;
            word_started = true;
            continue;
        }
        match character {
            '\\' if !single_quoted => {
                escaped = true;
                word_started = true;
            }
            '\'' if !double_quoted => {
                single_quoted = !single_quoted;
                word_started = true;
            }
            '"' if !single_quoted => {
                double_quoted = !double_quoted;
                word_started = true;
            }
            '#' if !single_quoted
                && !double_quoted
                && word_started
                && characters.peek().is_some_and(|next| *next == '{') =>
            {
                return true;
            }
            '#' if !single_quoted && !double_quoted && !word_started => {
                for character in characters.by_ref() {
                    if character == '\n' {
                        break;
                    }
                }
                word_started = false;
            }
            ';' | '\n' if !single_quoted && !double_quoted => word_started = false,
            character if character.is_whitespace() && !single_quoted && !double_quoted => {
                word_started = false;
            }
            _ => word_started = true,
        }
    }
    false
}

fn expand_stored_alias(
    engine: &MuxEngine,
    command: &mut CommandInvocation,
    aliases_available: bool,
) -> Result<bool, ServerError> {
    if !aliases_available {
        return Ok(false);
    }
    match engine.resolve_command_alias(command) {
        CommandAliasResolution::Miss => Ok(false),
        CommandAliasResolution::Expanded(expanded) => {
            *command = expanded;
            Ok(true)
        }
        CommandAliasResolution::MatchedUnsupported(_) => Err(ServerError::CommandParse(format!(
            "unknown command: {}",
            command.name
        ))),
    }
}

pub fn validate_static_command_chain(commands: &[CommandInvocation]) -> Result<(), ServerError> {
    for command in commands {
        validate_static_command(command)?;
    }
    Ok(())
}

fn validate_static_command(command: &CommandInvocation) -> Result<(), ServerError> {
    if let Some(commands) = parse_command_alias_group(command)? {
        return validate_static_command_chain(&commands);
    }
    let (name, spec) = match resolve_command(&command.name) {
        CommandResolution::Canonical(name) => (name, catalog_command_spec(name)),
        CommandResolution::Unimplemented(name) => (name, unimplemented_tmux_command_spec(name)),
        CommandResolution::Ambiguous(message) => return Err(ServerError::CommandParse(message)),
        CommandResolution::Unknown => {
            return Err(ServerError::CommandParse(format!(
                "unknown command: {}",
                command.name
            )));
        }
    };
    if NATIVE_COMMAND_NAMES.contains(&name) {
        return Ok(());
    }
    let Some(spec) = spec else {
        return Err(ServerError::CommandParse(format!(
            "unknown command: {}",
            command.name
        )));
    };
    let parsed = parse_tmux_command_options(spec, command)?;
    spec.validate_positional_minimum(parsed.positionals.len())?;
    spec.validate_positional_maximum(parsed.positionals.len())?;
    for index in 0..command.args.len() {
        if !command.argument_is_command_block(index) {
            continue;
        }
        let argument = &command.args[index];
        let body = crate::parser::command_block_body(argument).unwrap_or(argument);
        let parsed =
            crate::parser::parse_config_without_variable_expansion("<static-command-chain>", body);
        if let Some(diagnostic) = parsed.diagnostics.into_iter().next() {
            return Err(ServerError::CommandParse(diagnostic.message));
        }
        if name == "display-menu" {
            validate_menu_item_commands(&parsed.commands)?;
        } else {
            validate_static_command_chain(&parsed.commands)?;
        }
    }
    Ok(())
}

fn bind_key_command_tail(command: &CommandInvocation) -> Option<usize> {
    if canonical_command(&command.name) != "bind-key" {
        return None;
    }
    let spec = catalog_command_spec("bind-key")?;
    let parsed = parse_tmux_command_options(spec, command).ok()?;
    if parsed.positionals.len() < 2 {
        return None;
    }
    let tail = command
        .args
        .len()
        .saturating_sub(parsed.positionals.len())
        .saturating_add(1);
    (!command.argument_is_command_block(tail)).then_some(tail)
}

fn bind_key_menu_item_blocks(command: &CommandInvocation) -> Vec<usize> {
    let Some(tail) = bind_key_command_tail(command) else {
        return Vec::new();
    };
    let mut blocks = Vec::new();
    let mut inside_menu = false;
    let mut at_command_start = true;
    for index in tail..command.args.len() {
        let is_block = command.argument_is_command_block(index);
        if at_command_start {
            inside_menu = !is_block && canonical_command(&command.args[index]) == "display-menu";
            at_command_start = false;
        } else if is_block && inside_menu {
            blocks.push(index);
        }
        let word = command.args[index].to_string();
        if !is_block && word.ends_with(';') && !word.ends_with("\\;") {
            at_command_start = true;
        }
    }
    blocks
}

fn validate_menu_item_commands(commands: &[CommandInvocation]) -> Result<(), ServerError> {
    match validate_static_command_chain(commands) {
        Err(ServerError::UnsupportedCommand(_)) => Ok(()),
        result => result,
    }
}

fn validate_bound_command(command: &CommandInvocation, owner: &str) -> Result<(), ServerError> {
    if owner == "<menu-item>" {
        return validate_menu_item_commands(std::slice::from_ref(command));
    }
    let name = canonical_command(&command.name);
    if let Some(spec) = catalog_command_spec(name) {
        if spec.uses_tmux_option_grammar() {
            parse_tmux_command_options(spec, command)?;
        }
        let (options, positional) = parse_options_for_spec(&command.args, spec)?;
        spec.validate_positional_minimum(positional.len())?;
        if !NATIVE_COMMAND_NAMES.contains(&spec.name) {
            spec.validate_positional_maximum(positional.len())?;
        }
        validate_options(name, spec, &options)?;
        return Ok(());
    }
    if name == "copy-mode-repeat" {
        return Ok(());
    }
    if CommandSpec::UNIMPLEMENTED_TMUX_COMMANDS.contains(&name) {
        return Err(ServerError::UnsupportedCommand(format!("{owner} {name}")));
    }
    let message = match resolve_command(&command.name) {
        CommandResolution::Ambiguous(message) => message,
        _ => format!("unknown command: {name}"),
    };
    Err(ServerError::CommandParse(message))
}

pub(super) fn format_callback_commands(commands: &[CommandInvocation]) -> String {
    format_callback_commands_with(commands, format_command)
}

pub(super) fn format_callback_commands_round_trip(commands: &[CommandInvocation]) -> String {
    format_callback_commands_with(commands, command_round_trip_print)
}

fn format_callback_commands_with(
    commands: &[CommandInvocation],
    render: impl Fn(&CommandInvocation) -> String,
) -> String {
    let mut output = String::new();
    let mut previous_group = None;
    for command in commands {
        let group = command
            .source
            .as_ref()
            .map(|source| (&source.source, source.line));
        if !output.is_empty() {
            if previous_group
                .zip(group)
                .is_some_and(|(previous, current)| previous != current)
            {
                output.push('\n');
            } else {
                output.push_str(" ; ");
            }
        }
        output.push_str(&render(command));
        previous_group = group;
    }
    output
}

fn command_round_trip_print(command: &CommandInvocation) -> String {
    if let Some(body) = command_alias_group_body(command) {
        return body.to_owned();
    }
    let mut output = tmux_args_escape(canonical_command(&command.name));
    if !command.args.is_empty() {
        output.push(' ');
        output.push_str(
            &command
                .args
                .iter()
                .enumerate()
                .map(|(index, argument)| {
                    if command.argument_is_command_block(index) {
                        argument.to_string()
                    } else {
                        tmux_args_escape_bytes(argument.as_bytes())
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    output
}

pub fn format_command(command: &CommandInvocation) -> String {
    tmux_command_print(command)
}

pub(crate) fn format_key_command(binding: &Binding) -> String {
    binding
        .commands
        .iter()
        .flat_map(|command| match parse_command_alias_group(command) {
            Ok(Some(commands)) => commands.iter().map(tmux_command_print).collect::<Vec<_>>(),
            _ => vec![tmux_command_print(command)],
        })
        .collect::<Vec<_>>()
        .join(" \\; ")
}

pub(super) fn command_alias_group_body(command: &CommandInvocation) -> Option<&str> {
    (command.is_expanded_alias_group()
        && command.args.len() == 1
        && command.argument_is_command_block(0))
    .then(|| crate::parser::command_block_body(&command.args[0]).map(str::trim))
    .flatten()
}

pub(super) fn parse_command_alias_group(
    command: &CommandInvocation,
) -> Result<Option<Vec<CommandInvocation>>, ServerError> {
    let Some(body) = command_alias_group_body(command) else {
        return Ok(None);
    };
    let parsed = crate::parser::parse_config_without_variable_expansion("<command-alias>", body);
    if let Some(diagnostic) = parsed.diagnostics.into_iter().next() {
        return Err(ServerError::CommandParse(diagnostic.message));
    }
    Ok(Some(parsed.commands))
}

pub(super) fn tmux_command_print(command: &CommandInvocation) -> String {
    if let Some(body) = command_alias_group_body(command) {
        return body.to_owned();
    }
    let name = canonical_command(&command.name);
    let spec = catalog_command_spec(name);
    let Some(spec) = spec else {
        return std::iter::once(name)
            .chain(command.args.iter().map(RawText::as_str))
            .map(tmux_args_escape)
            .collect::<Vec<_>>()
            .join(" ");
    };
    let option_for = |flag: char| {
        spec.options.iter().find(|option| {
            option
                .name
                .strip_prefix('-')
                .is_some_and(|rest| rest.chars().eq(std::iter::once(flag)))
        })
    };

    let mut flags = BTreeMap::<char, usize>::new();
    let mut long_flags = BTreeMap::<&str, usize>::new();
    let mut valued = BTreeMap::<char, Vec<(usize, &str)>>::new();
    let mut long_valued = BTreeMap::<&str, Vec<(usize, &str)>>::new();
    let mut positional = Vec::new();
    let mut args = command
        .args
        .iter()
        .enumerate()
        .map(|(index, value)| (index, value.as_str()));
    let mut parsing_flags = true;
    while let Some((index, arg)) = args.next() {
        if !parsing_flags {
            positional.push((index, arg));
            continue;
        }
        if arg == "--" {
            parsing_flags = false;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, attached) = long
                .split_once('=')
                .map_or((long, None), |(name, value)| (name, Some(value)));
            let option = spec
                .options
                .iter()
                .find(|option| option.name.strip_prefix("--") == Some(name));
            match (option, attached) {
                (Some(option), Some(value)) if option.value.is_some() && !value.is_empty() => {
                    long_valued
                        .entry(option.name)
                        .or_default()
                        .push((index, value));
                }
                (Some(option), None) if option.value.is_some() => {
                    let value = args.next().unwrap_or((index, ""));
                    long_valued.entry(option.name).or_default().push(value);
                }
                (Some(option), None) => *long_flags.entry(option.name).or_default() += 1,
                _ => {
                    parsing_flags = false;
                    positional.push((index, arg));
                }
            }
            continue;
        }
        let Some(cluster) = arg.strip_prefix('-').filter(|rest| !rest.is_empty()) else {
            parsing_flags = false;
            positional.push((index, arg));
            continue;
        };
        for (offset, flag) in cluster.char_indices() {
            let rest = &cluster[offset + flag.len_utf8()..];
            match option_for(flag) {
                Some(option) if option.attached_value => {
                    if rest.is_empty() {
                        *flags.entry(flag).or_default() += 1;
                    } else {
                        valued.entry(flag).or_default().push((index, rest));
                    }
                    break;
                }
                Some(option) if option.value.is_some() => {
                    let value = if rest.is_empty() {
                        args.next().unwrap_or((index, ""))
                    } else {
                        (index, rest)
                    };
                    valued.entry(flag).or_default().push(value);
                    break;
                }
                _ => *flags.entry(flag).or_default() += 1,
            }
        }
    }

    let mut output = name.to_owned();
    if !flags.is_empty() {
        output.push_str(" -");
        for (flag, count) in &flags {
            output.extend(std::iter::repeat_n(*flag, *count));
        }
    }
    for (flag, count) in &long_flags {
        for _ in 0..*count {
            output.push(' ');
            output.push_str(flag);
        }
    }
    for (flag, values) in &valued {
        for (index, value) in values {
            output.push_str(" -");
            output.push(*flag);
            output.push(' ');
            output.push_str(&tmux_command_argument_print(command, *index, value));
        }
    }
    for (flag, values) in &long_valued {
        for (index, value) in values {
            output.push(' ');
            output.push_str(flag);
            output.push(' ');
            output.push_str(&tmux_command_argument_print(command, *index, value));
        }
    }
    for (index, arg) in positional {
        output.push(' ');
        output.push_str(&tmux_command_argument_print(command, index, arg));
    }
    output
}

fn tmux_command_argument_print(command: &CommandInvocation, index: usize, value: &str) -> String {
    if command.argument_is_command_block(index) {
        value.replace('\n', " ;; ")
    } else {
        tmux_args_escape(value)
    }
}

pub(super) fn format_command_arguments(command: &CommandInvocation) -> String {
    command
        .args
        .iter()
        .enumerate()
        .map(|(index, argument)| tmux_command_argument_print(command, index, argument))
        .collect::<Vec<_>>()
        .join(" ")
}
