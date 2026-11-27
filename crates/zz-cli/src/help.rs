use super::*;

pub(super) fn top_level_help() -> String {
    use std::fmt::Write as _;

    let mut help = String::from(concat!(
        "usage: zz [global flags] <command> [flags]\n\n",
        "global flags\n",
        "  --host NAME|DEST|all  run on a host from zz list-hosts, an ssh destination, or every host (read-only verbs)\n",
        "  -L NAME, -S PATH, -f FILE  tmux socket and config flags; zz -h lists the rest\n",
        "\nzz verbs\n",
    ));
    for name in zz_protocol::NATIVE_COMMAND_NAMES {
        if let Some(spec) = catalog_command_spec(name)
            && !spec.is_internal()
        {
            let _ = writeln!(help, "  {}  {}", spec.name, spec.description);
        }
    }
    help.push_str("\ntmux commands\n");
    for spec in zz_protocol::command_specs().filter(|spec| {
        !spec.is_internal() && !zz_protocol::NATIVE_COMMAND_NAMES.contains(&spec.name)
    }) {
        let _ = writeln!(help, "  {}{}", spec.name, help_aliases(spec));
    }
    help.push_str("\nnot implemented\n");
    for name in zz_protocol::CommandSpec::UNIMPLEMENTED_TMUX_COMMANDS {
        let _ = writeln!(help, "  {name}");
    }
    help.push_str("\nRun zz tools for the agent guide.\n");
    help
}

fn help_aliases(spec: &zz_protocol::CommandSpec) -> String {
    if spec.aliases.is_empty() {
        String::new()
    } else {
        format!(" ({})", spec.aliases.join(", "))
    }
}

fn help_value(kind: zz_protocol::CommandValueKind) -> &'static str {
    use zz_protocol::CommandValueKind;

    match kind {
        CommandValueKind::FreeForm => "value",
        CommandValueKind::Session => "session",
        CommandValueKind::Window => "window",
        CommandValueKind::Pane => "pane",
        CommandValueKind::Layout => "layout",
        CommandValueKind::PaneKind => "pane-kind",
        CommandValueKind::KeyTable => "key-table",
        CommandValueKind::SetOption => "option",
        CommandValueKind::Boolean => "boolean",
    }
}

pub(super) fn command_help(command: &str) -> Result<String, ServerError> {
    use std::fmt::Write as _;

    let spec = match zz_protocol::resolve_command(command) {
        zz_protocol::CommandResolution::Canonical(name) => catalog_command_spec(name),
        zz_protocol::CommandResolution::Unimplemented(name) => {
            zz_protocol::unimplemented_tmux_command_spec(name)
        }
        zz_protocol::CommandResolution::Ambiguous(message) => {
            return Err(ServerError::NativeCommandParse(message));
        }
        zz_protocol::CommandResolution::Unknown => None,
    }
    .ok_or_else(|| ServerError::NativeCommandParse(format!("unknown command: {command}")))?;
    let mut help = format!(
        "{}{}\n{}\nusage: zz {} {}\n",
        spec.name,
        help_aliases(spec),
        spec.description,
        spec.name,
        spec.usage,
    );
    if !spec.options.is_empty() {
        help.push_str("\noptions\n");
    }
    for option in spec.options {
        let value = if option.optional_value {
            " [value]".to_owned()
        } else {
            option
                .value
                .map(|kind| format!(" <{}>", help_value(kind)))
                .unwrap_or_default()
        };
        let unsupported = if option.unsupported {
            " (not supported)"
        } else {
            ""
        };
        let _ = writeln!(
            help,
            "  {}{value}  {}{unsupported}",
            option.name, option.description
        );
    }
    let variadic = spec
        .variadic
        .filter(|kind| *kind != zz_protocol::CommandValueKind::FreeForm);
    if !spec.positionals.is_empty() || variadic.is_some() {
        help.push_str("\npositionals\n");
        let names = usage_positional_names(spec.usage);
        for (index, kind) in spec.positionals.iter().enumerate() {
            match names.as_ref().and_then(|names| names.get(index)) {
                Some(name) => {
                    let _ = writeln!(help, "  <{name}>");
                }
                None => {
                    let _ = writeln!(help, "  <{}>", help_value(*kind));
                }
            }
        }
        if let Some(kind) = variadic {
            let _ = writeln!(help, "  <{}>...", help_value(kind));
        }
    }
    Ok(help)
}

fn usage_positional_names(usage: &str) -> Option<Vec<&str>> {
    let mut depth = 0usize;
    let mut names = Vec::new();
    for token in usage.split_whitespace() {
        let opens = token.matches('[').count();
        let closes = token.matches(']').count();
        if depth == 0 && opens == 0 && !token.starts_with('-') && !token.ends_with("...") {
            names.push(token);
        }
        depth = (depth + opens).saturating_sub(closes);
    }
    (!names.is_empty()).then_some(names)
}
