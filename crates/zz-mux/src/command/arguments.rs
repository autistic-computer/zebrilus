use super::*;

pub(super) fn tmux_flag(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

pub(super) fn parse_command_options(
    command: &str,
    args: &[RawText],
) -> Result<(Options, Vec<RawText>), ServerError> {
    let spec = command_spec(command).expect("executable command has catalog metadata");
    let (options, positional) = parse_options_for_spec(args, spec)?;
    spec.validate_positional_minimum(positional.len())?;
    if !NATIVE_COMMAND_NAMES.contains(&spec.name) {
        spec.validate_positional_maximum(positional.len())?;
    }
    validate_options(command, spec, &options)?;
    Ok((options, positional))
}

pub(super) fn parse_options_for_spec(
    args: &[RawText],
    spec: &zz_protocol::CommandSpec,
) -> Result<(Options, Vec<RawText>), ServerError> {
    if spec.uses_tmux_option_grammar() {
        let parsed = parse_tmux_options(spec, args)?;
        let mut options = Options::default();
        for option in parsed.options {
            match option {
                zz_protocol::TmuxOption::Flag(name) => options.flags.push(name.to_owned()),
                zz_protocol::TmuxOption::Value(name, value) => {
                    options.values.push((name.to_owned(), value.to_owned()));
                }
            }
        }
        return Ok((options, parsed.positionals.to_vec()));
    }
    let value_options = spec
        .options
        .iter()
        .filter_map(|option| option.value.map(|_| option.name))
        .collect::<Vec<_>>();
    let attached_options = spec
        .options
        .iter()
        .filter_map(|option| option.attached_value.then_some(option.name))
        .collect::<Vec<_>>();
    parse_options(args, &value_options, &attached_options)
        .map_err(|error| spec.classify_usage_error(error))
}

pub(super) fn validate_options(
    command: &str,
    spec: &zz_protocol::CommandSpec,
    options: &Options,
) -> Result<(), ServerError> {
    validate_options_allowing_unsupported(command, spec, options, None)
}

pub(super) fn validate_options_allowing_unsupported(
    command: &str,
    spec: &zz_protocol::CommandSpec,
    options: &Options,
    allowed: Option<&str>,
) -> Result<(), ServerError> {
    for name in options
        .flags
        .iter()
        .map(String::as_str)
        .chain(options.values.iter().map(|(name, _)| name.as_str()))
    {
        let Some(option) = spec.option(name) else {
            return Err(spec.parse_error(format!("{command} does not support {name}")));
        };
        if option.unsupported && allowed != Some(name) {
            return Err(ServerError::UnsupportedCommand(format!("{command} {name}")));
        }
    }
    Ok(())
}

pub(super) fn parse_options(
    args: &[RawText],
    value_options: &[&str],
    attached_options: &[&str],
) -> Result<(Options, Vec<RawText>), ServerError> {
    let mut options = Options::default();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if !argument.starts_with('-') || argument == "-" {
            break;
        }
        index += 1;
        if argument == "--" {
            break;
        }
        if let Some(long) = argument.strip_prefix("--") {
            let (name, attached) = long
                .split_once('=')
                .map_or((long, None), |(name, value)| (name, Some(value)));
            let name = format!("--{name}");
            if value_options.contains(&name.as_str()) {
                let value = match attached {
                    Some(value) if !value.is_empty() => value.to_owned(),
                    Some(_) => {
                        options.flags.push(argument.to_string());
                        continue;
                    }
                    None => {
                        let value = required_arg(args, index, &name)?.to_owned();
                        index += 1;
                        value
                    }
                };
                options.values.push((name, value));
            } else {
                options.flags.push(argument.to_string());
            }
            continue;
        }
        let mut cluster = argument[1..].chars();
        while let Some(character) = cluster.next() {
            let name = format!("-{character}");
            if attached_options.contains(&name.as_str()) {
                let attached = cluster.as_str();
                if attached.is_empty() {
                    options.flags.push(name);
                } else {
                    options.values.push((name, attached.to_owned()));
                }
                break;
            }
            if !value_options.contains(&name.as_str()) {
                options.flags.push(name);
                continue;
            }
            let attached = cluster.as_str();
            let value = if attached.is_empty() {
                let value = required_arg(args, index, &name)?.to_owned();
                index += 1;
                value
            } else {
                attached.to_owned()
            };
            options.values.push((name, value));
            break;
        }
    }
    Ok((options, args[index..].to_vec()))
}

pub(super) fn required_arg<'a>(
    args: &'a [RawText],
    index: usize,
    option: &str,
) -> Result<&'a str, ServerError> {
    args.get(index)
        .map(RawText::as_str)
        .ok_or_else(|| ServerError::CommandParse(format!("{option} requires an argument")))
}

pub(super) fn reject_positionals(command: &str, positional: &[RawText]) -> Result<(), ServerError> {
    if let Some(argument) = positional.first() {
        Err(ServerError::CommandParse(format!(
            "{command} does not take positional arguments: {argument}"
        )))
    } else {
        Ok(())
    }
}

pub(super) fn percentage_value(
    value: &str,
    minimum: i64,
    maximum: i64,
    current: i64,
) -> Result<i64, String> {
    let strtonum = |value: &str, minimum: i64, maximum: i64| {
        parse_strtonum(value, minimum, maximum, "").map_err(|error| match error {
            ServerError::InvalidCommand(cause) => cause.trim_start().to_owned(),
            other => other.to_string(),
        })
    };
    if let Some(percent) = value.strip_suffix('%') {
        let value = current * strtonum(percent, 0, 1000)? / 100;
        if value < minimum {
            return Err("too small".to_owned());
        }
        if value > maximum {
            return Err("too large".to_owned());
        }
        return Ok(value);
    }
    strtonum(value, minimum, maximum)
}

pub(super) fn parse_strtonum(
    value: &str,
    minimum: i64,
    maximum: i64,
    label: &str,
) -> Result<i64, ServerError> {
    let trimmed = value.trim_start();
    let (negative, digits) = trimmed.strip_prefix('-').map_or_else(
        || (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
        |digits| (true, digits),
    );
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ServerError::InvalidCommand(format!("{label} invalid")));
    }
    let magnitude = digits.parse::<i128>().unwrap_or(i128::MAX);
    let number = if negative { -magnitude } else { magnitude };
    if number < i128::from(minimum) {
        return Err(ServerError::InvalidCommand(format!("{label} too small")));
    }
    if number > i128::from(maximum) {
        return Err(ServerError::InvalidCommand(format!("{label} too large")));
    }
    Ok(i64::try_from(number).expect("the value was bounded to i64 limits"))
}

pub(super) fn parse_resize_adjustment(value: &str) -> Result<i32, ServerError> {
    value
        .parse::<i32>()
        .map_err(|_| ServerError::InvalidCommand("adjustment invalid".to_owned()))
}

pub(super) fn parse_resize_window_adjustment(value: &str) -> Result<u32, ServerError> {
    parse_strtonum(value, 1, i64::from(i32::MAX), "adjustment")
        .map(|value| u32::try_from(value).expect("window adjustment is positive and bounded"))
}

pub(super) fn parse_window_extent(value: &str, dimension: &str) -> Result<u16, ServerError> {
    parse_strtonum(value, 1, i64::from(PANE_MAXIMUM), dimension)
        .map(|value| u16::try_from(value).expect("window extent is bounded to u16"))
}

pub(super) fn parse_resize_size(
    value: &str,
    extent: u16,
    dimension: &str,
) -> Result<u16, ServerError> {
    let (number, maximum) = value
        .strip_suffix('%')
        .map_or((value, i64::from(PANE_MAXIMUM)), |percentage| {
            (percentage, 1_000)
        });
    let number = number
        .parse::<i64>()
        .map_err(|_| ServerError::InvalidCommand(format!("{dimension} invalid")))?;
    if number < 0 {
        return Err(ServerError::InvalidCommand(format!(
            "{dimension} too small"
        )));
    }
    if number > maximum {
        return Err(ServerError::InvalidCommand(format!(
            "{dimension} too large"
        )));
    }
    let cells = if value.ends_with('%') {
        u64::from(extent) * u64::try_from(number).expect("percentage is nonnegative") / 100
    } else {
        u64::try_from(number).expect("pane size is nonnegative")
    };
    u16::try_from(cells)
        .ok()
        .filter(|cells| *cells <= PANE_MAXIMUM)
        .ok_or_else(|| ServerError::InvalidCommand(format!("{dimension} too large")))
}

pub(super) fn exactly_one_argument<'a>(
    command: &str,
    positional: &'a [RawText],
) -> Result<&'a str, ServerError> {
    if positional.len() == 1 {
        Ok(positional[0].as_str())
    } else {
        Err(ServerError::CommandParse(format!(
            "{command} requires exactly one new name"
        )))
    }
}
