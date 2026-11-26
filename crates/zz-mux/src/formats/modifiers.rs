use super::*;

pub(super) fn parse_modifiers(
    body: &str,
    mut argument: impl FnMut(&str) -> String,
) -> Option<(Vec<Modifier>, usize)> {
    let mut modifiers = Vec::new();
    let mut position = 0usize;
    while position < body.len() && body.as_bytes()[position] != b':' {
        if body.as_bytes()[position] == b';' {
            position += 1;
        }
        if position >= body.len() {
            return None;
        }
        let remaining = &body[position..];
        let mut matched = None;
        for spec in FORMAT_MODIFIER_SPECS
            .iter()
            .filter(|spec| spec.text.len() > 1)
        {
            if remaining.starts_with(spec.text)
                && remaining
                    .as_bytes()
                    .get(spec.text.len())
                    .is_some_and(|next| is_modifier_end(*next))
            {
                matched = Some((spec.kind, spec.text));
                break;
            }
        }
        if let Some((kind, text)) = matched {
            modifiers.push(Modifier {
                kind,
                text,
                args: Vec::new(),
            });
            position += text.len();
            continue;
        }
        let character = remaining.as_bytes()[0];
        let next = remaining.as_bytes().get(1).copied();
        let spec = FORMAT_MODIFIER_SPECS
            .iter()
            .find(|spec| spec.text.as_bytes() == [character])?;
        if next.is_some_and(is_modifier_end) {
            modifiers.push(Modifier {
                kind: spec.kind,
                text: spec.text,
                args: Vec::new(),
            });
            position += 1;
            continue;
        }
        if !spec.arguments {
            return None;
        }
        let kind = spec.kind;
        let text = spec.text;
        position += 1;
        if position >= body.len() {
            return None;
        }
        if is_modifier_end(body.as_bytes()[position]) {
            modifiers.push(Modifier {
                kind,
                text,
                args: Vec::new(),
            });
            continue;
        }
        let wrapper = body.as_bytes()[position];
        let mut args = Vec::new();
        if wrapper.is_ascii_punctuation() && wrapper != b'-' {
            loop {
                if body.as_bytes().get(position) == Some(&wrapper)
                    && body
                        .as_bytes()
                        .get(position + 1)
                        .is_some_and(|next| is_modifier_end(*next))
                {
                    position += 1;
                    break;
                }
                let start = position + 1;
                let end = find_modifier_argument(body, start, wrapper)?;
                let value = unescape(&body[start..end]);
                args.push(argument(&value));
                position = end;
                if is_modifier_end(body.as_bytes()[position]) {
                    break;
                }
            }
        } else {
            let end = find_modifier_argument(body, position, 0)?;
            let value = unescape(&body[position..end]);
            args.push(argument(&value));
            position = end;
        }
        modifiers.push(Modifier { kind, text, args });
    }
    if body.as_bytes().get(position) == Some(&b':') {
        Some((modifiers, position + 1))
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ModifierKind {
    Literal,
    Basename,
    Dirname,
    Length,
    Width,
    Expand,
    ExpandTime,
    Character,
    Not,
    NotNot,
    And,
    Or,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    Match,
    Time,
    Quote,
    Substitute,
    Limit,
    Padding,
    Expression,
    Colour,
    NameExists,
    Sessions,
    Windows,
    Panes,
    Clients,
    Options,
    Environments,
    ContentSearch,
    Repeat,
    Interrogate,
    Animate,
}

impl FormatModifierSpec {
    const fn new(text: &'static str, kind: ModifierKind, arguments: bool) -> Self {
        Self {
            text,
            kind,
            arguments,
        }
    }
}

const FORMAT_MODIFIER_SPECS: [FormatModifierSpec; 37] = [
    FormatModifierSpec::new("||", ModifierKind::Or, false),
    FormatModifierSpec::new("&&", ModifierKind::And, false),
    FormatModifierSpec::new("!!", ModifierKind::NotNot, false),
    FormatModifierSpec::new("!=", ModifierKind::NotEqual, false),
    FormatModifierSpec::new("==", ModifierKind::Equal, false),
    FormatModifierSpec::new("<=", ModifierKind::LessEqual, false),
    FormatModifierSpec::new(">=", ModifierKind::GreaterEqual, false),
    FormatModifierSpec::new("l", ModifierKind::Literal, false),
    FormatModifierSpec::new("b", ModifierKind::Basename, false),
    FormatModifierSpec::new("d", ModifierKind::Dirname, false),
    FormatModifierSpec::new("n", ModifierKind::Length, false),
    FormatModifierSpec::new("w", ModifierKind::Width, false),
    FormatModifierSpec::new("E", ModifierKind::Expand, false),
    FormatModifierSpec::new("T", ModifierKind::ExpandTime, false),
    FormatModifierSpec::new("a", ModifierKind::Character, true),
    FormatModifierSpec::new("!", ModifierKind::Not, false),
    FormatModifierSpec::new("<", ModifierKind::Less, false),
    FormatModifierSpec::new(">", ModifierKind::Greater, false),
    FormatModifierSpec::new("m", ModifierKind::Match, true),
    FormatModifierSpec::new("s", ModifierKind::Substitute, true),
    FormatModifierSpec::new("t", ModifierKind::Time, true),
    FormatModifierSpec::new("=", ModifierKind::Limit, true),
    FormatModifierSpec::new("q", ModifierKind::Quote, true),
    FormatModifierSpec::new("p", ModifierKind::Padding, true),
    FormatModifierSpec::new("e", ModifierKind::Expression, true),
    FormatModifierSpec::new("c", ModifierKind::Colour, true),
    FormatModifierSpec::new("N", ModifierKind::NameExists, true),
    FormatModifierSpec::new("S", ModifierKind::Sessions, true),
    FormatModifierSpec::new("W", ModifierKind::Windows, true),
    FormatModifierSpec::new("P", ModifierKind::Panes, true),
    FormatModifierSpec::new("L", ModifierKind::Clients, true),
    FormatModifierSpec::new("O", ModifierKind::Options, true),
    FormatModifierSpec::new("V", ModifierKind::Environments, true),
    FormatModifierSpec::new("C", ModifierKind::ContentSearch, true),
    FormatModifierSpec::new("R", ModifierKind::Repeat, true),
    FormatModifierSpec::new("I", ModifierKind::Interrogate, true),
    FormatModifierSpec::new("A", ModifierKind::Animate, true),
];

#[cfg(test)]
pub(crate) fn format_modifier_names() -> impl Iterator<Item = &'static str> {
    FORMAT_MODIFIER_SPECS.iter().map(|spec| spec.text)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LoopTarget {
    Sessions,
    Windows,
    Panes,
}

impl LoopTarget {
    pub(super) const fn format_type(self) -> FormatType {
        match self {
            Self::Sessions => FormatType::Session,
            Self::Windows => FormatType::Window,
            Self::Panes => FormatType::Pane,
        }
    }
}

impl FormatVariables for LoopVariables<'_> {
    fn variable(&self, name: &str) -> Option<Cow<'_, str>> {
        if let Some(value) = self.dynamic.get(name) {
            return Some(Cow::Borrowed(value));
        }
        let spec = format_variable(name)?;
        Some(self.context.resolve(spec, self.format_type))
    }

    fn variable_kind(&self, name: &str) -> Option<FormatKind> {
        if self.dynamic.contains_key(name) {
            Some(FormatKind::String)
        } else {
            format_variable(name).map(|spec| spec.kind)
        }
    }

    fn values(&self) -> &StatusContext<'_> {
        self.context
    }

    fn format_type(&self) -> FormatType {
        self.format_type
    }
}

#[derive(Clone, Copy)]
pub(super) enum Comparison<'a> {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    Match(&'a str),
}

pub(super) const fn comparison_text(comparison: Comparison<'_>) -> &'static str {
    match comparison {
        Comparison::Equal => "==",
        Comparison::NotEqual => "!=",
        Comparison::Less => "<",
        Comparison::Greater => ">",
        Comparison::LessEqual => "<=",
        Comparison::GreaterEqual => ">=",
        Comparison::Match(_) => "m",
    }
}

#[derive(Clone, Copy)]
pub(super) enum LoopSort {
    Index,
    Name,
    Activity,
    Order,
    Creation,
    Z,
}

impl<'a> ModifierFlags<'a> {
    pub(super) fn from_modifiers(modifiers: &'a [Modifier]) -> Self {
        let mut flags = Self::default();
        for modifier in modifiers {
            match modifier.kind {
                ModifierKind::Literal => flags.literal = true,
                ModifierKind::Basename => flags.basename = true,
                ModifierKind::Dirname => flags.dirname = true,
                ModifierKind::Length => flags.length = true,
                ModifierKind::Width => flags.width = true,
                ModifierKind::Expand => flags.expand = true,
                ModifierKind::ExpandTime => flags.expand_time = true,
                ModifierKind::Character => flags.character = true,
                ModifierKind::Not => flags.not = true,
                ModifierKind::NotNot => flags.not_not = true,
                ModifierKind::And => flags.bool_op = Some(true),
                ModifierKind::Or => flags.bool_op = Some(false),
                ModifierKind::Equal => flags.comparison = Some(Comparison::Equal),
                ModifierKind::NotEqual => flags.comparison = Some(Comparison::NotEqual),
                ModifierKind::Less => flags.comparison = Some(Comparison::Less),
                ModifierKind::Greater => flags.comparison = Some(Comparison::Greater),
                ModifierKind::LessEqual => flags.comparison = Some(Comparison::LessEqual),
                ModifierKind::GreaterEqual => flags.comparison = Some(Comparison::GreaterEqual),
                ModifierKind::Match => {
                    flags.comparison = Some(Comparison::Match(
                        modifier.args.first().map_or("", String::as_str),
                    ));
                }
                ModifierKind::Time => {
                    flags.time.enabled = true;
                    if let Some(time_flags) = modifier.args.first() {
                        if time_flags.contains('p') {
                            flags.time.pretty = true;
                        } else if time_flags.contains('r') {
                            flags.time.relative = true;
                        } else if time_flags.contains('d') {
                            flags.time.difference = true;
                        } else if time_flags.contains('f')
                            && let Some(format) = modifier.args.get(1)
                        {
                            flags.time.format = Some(format.as_str());
                        }
                    }
                }
                ModifierKind::Quote => {
                    let Some(quote_flags) = modifier.args.first() else {
                        flags.quote_shell = true;
                        continue;
                    };
                    if quote_flags.contains('s') {
                        flags.quote_single = true;
                    } else if quote_flags.contains('e') || quote_flags.contains('h') {
                        flags.quote_style = true;
                    } else if quote_flags.contains('a') {
                        flags.quote_arguments = true;
                    }
                }
                ModifierKind::Substitute if modifier.args.len() >= 2 => {
                    flags.substitutions.push(modifier);
                }
                ModifierKind::Limit => {
                    if let Some(limit) = modifier.args.first() {
                        let limit = limit
                            .parse::<isize>()
                            .ok()
                            .filter(|limit| (-FORMAT_MAX_WIDTH..=FORMAT_MAX_WIDTH).contains(limit))
                            .unwrap_or_default();
                        let marker = modifier
                            .args
                            .get(1)
                            .map(String::as_str)
                            .or_else(|| flags.limit.as_ref().and_then(|(_, marker)| *marker));
                        flags.limit = Some((limit, marker));
                    }
                }
                ModifierKind::Padding => {
                    flags.padding = modifier
                        .args
                        .first()
                        .and_then(|width| width.parse::<isize>().ok())
                        .filter(|width| (-FORMAT_MAX_WIDTH..=FORMAT_MAX_WIDTH).contains(width))
                        .unwrap_or_default();
                }
                ModifierKind::Expression if (1..=3).contains(&modifier.args.len()) => {
                    flags.expression = Some(modifier);
                }
                ModifierKind::Substitute | ModifierKind::Expression => {}
                ModifierKind::Colour => {
                    flags.colour = Some(modifier.args.first().map_or("", String::as_str));
                }
                ModifierKind::Interrogate => {
                    if let Some(word) = modifier.args.first()
                        && word.contains(['c', 'f', 'e'])
                    {
                        flags.interrogate = Some(word.as_str());
                    }
                }
                ModifierKind::NameExists => {
                    if modifier
                        .args
                        .first()
                        .is_none_or(|value| value.contains('w'))
                    {
                        flags.name_window = true;
                    } else if modifier
                        .args
                        .first()
                        .is_some_and(|value| value.contains('s'))
                    {
                        flags.name_session = true;
                    }
                }
                ModifierKind::Sessions => {
                    flags.loop_sessions = true;
                    let value = modifier.args.first().map_or("", String::as_str);
                    flags.loop_sort = Some(if value.contains('i') {
                        LoopSort::Index
                    } else if value.contains('n') {
                        LoopSort::Name
                    } else if value.contains('t') {
                        LoopSort::Activity
                    } else {
                        LoopSort::Index
                    });
                    flags.loop_reversed = value.contains('r');
                }
                ModifierKind::Windows => {
                    flags.loop_windows = true;
                    let value = modifier.args.first().map_or("", String::as_str);
                    flags.loop_sort = Some(if value.contains('i') {
                        LoopSort::Order
                    } else if value.contains('n') {
                        LoopSort::Name
                    } else if value.contains('t') {
                        LoopSort::Activity
                    } else {
                        LoopSort::Order
                    });
                    flags.loop_reversed = value.contains('r');
                }
                ModifierKind::Panes => {
                    flags.loop_panes = true;
                    let value = modifier.args.first().map_or("", String::as_str);
                    flags.loop_sort = Some(if value.contains('i') {
                        LoopSort::Index
                    } else if value.contains('z') {
                        LoopSort::Z
                    } else {
                        LoopSort::Creation
                    });
                    flags.loop_reversed = value.contains('r');
                }
                ModifierKind::Clients => {
                    flags.loop_clients = true;
                    let value = modifier.args.first().map_or("", String::as_str);
                    flags.loop_sort = Some(if value.contains('i') {
                        LoopSort::Order
                    } else if value.contains('n') {
                        LoopSort::Name
                    } else if value.contains('t') {
                        LoopSort::Activity
                    } else {
                        LoopSort::Order
                    });
                    flags.loop_reversed = value.contains('r');
                }
                ModifierKind::Options => {
                    flags.loop_options = Some(modifier.args.first().map_or("", String::as_str));
                }
                ModifierKind::Environments => {
                    flags.loop_environment = Some(modifier.args.first().map_or("", String::as_str));
                }
                ModifierKind::ContentSearch => {
                    flags.content_search = Some(modifier.args.first().map_or("", String::as_str));
                }
                ModifierKind::Repeat => flags.repeat = true,
                ModifierKind::Animate => {
                    flags.cycle = Some(
                        modifier
                            .args
                            .first()
                            .and_then(|count| count.parse::<u64>().ok())
                            .filter(|count| (1..=100).contains(count))
                            .unwrap_or(1),
                    );
                }
            }
        }
        flags
    }
}

pub(super) fn insert_window_neighbour_user_options(
    dynamic: &mut BTreeMap<String, String>,
    universe: &FormatUniverseRef<'_>,
    neighbour: &FormatLoopItem,
    prefix: &str,
) {
    let Some(options) = universe.window_user_options(&neighbour.context.window_id) else {
        return;
    };
    for (name, value) in options.iter() {
        dynamic.insert(format!("{prefix}{name}"), value.clone());
    }
}

pub(super) fn client_loop_scoped(name: &str) -> bool {
    format_variable(name).is_some_and(|spec| {
        spec.scope == FormatScope::Client && spec.backing == FormatBacking::StatusHook
    })
}

pub(super) fn sort_client_loop_rows(
    rows: &mut [FormatClientRow],
    sort: Option<LoopSort>,
    reversed: bool,
) {
    if !matches!(sort, Some(LoopSort::Name | LoopSort::Activity)) {
        if reversed {
            rows.reverse();
        }
        return;
    }
    rows.sort_by(|left, right| {
        let ordering = match sort {
            Some(LoopSort::Activity) => right.activity.cmp(&left.activity),
            _ => Ordering::Equal,
        }
        .then_with(|| left.name.cmp(&right.name));
        if reversed {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

pub(super) fn sort_loop_items(
    items: &mut [&FormatLoopItem],
    target: LoopTarget,
    sort: Option<LoopSort>,
) {
    match (target, sort) {
        (LoopTarget::Sessions, Some(LoopSort::Name)) => items.sort_by(|left, right| {
            left.metadata
                .session_name
                .cmp(&right.metadata.session_name)
                .then_with(|| {
                    session_number(&left.context.session_id)
                        .cmp(&session_number(&right.context.session_id))
                })
        }),
        (LoopTarget::Sessions, Some(LoopSort::Activity)) => items.sort_by(|left, right| {
            right
                .metadata
                .session_sort_activity
                .cmp(&left.metadata.session_sort_activity)
                .then_with(|| left.metadata.session_name.cmp(&right.metadata.session_name))
        }),
        (LoopTarget::Sessions, _) => {
            items.sort_by_key(|item| session_number(&item.context.session_id));
        }
        (LoopTarget::Windows, Some(LoopSort::Name)) => items.sort_by(|left, right| {
            left.metadata
                .window_name
                .cmp(&right.metadata.window_name)
                .then_with(|| left.metadata.window_index.cmp(&right.metadata.window_index))
        }),
        (LoopTarget::Windows, _) => items.sort_by_key(|item| item.metadata.window_index),
        (LoopTarget::Panes, Some(LoopSort::Index)) => {
            items.sort_by_key(|item| item.metadata.pane_index);
        }
        (LoopTarget::Panes, Some(LoopSort::Z)) => items.sort_by_key(|item| {
            (
                item.metadata.pane_z.unwrap_or_default(),
                pane_number(&item.context.pane_id),
            )
        }),
        (LoopTarget::Panes, _) => items.sort_by_key(|item| pane_number(&item.context.pane_id)),
    }
}

pub(super) fn option_loop_key(loop_flags: &str, values: &StatusContext) -> Option<OptionRowsKey> {
    let loop_flags = if loop_flags.is_empty() {
        "s"
    } else {
        loop_flags
    };
    if loop_flags.contains('v') {
        return Some(OptionRowsKey::Server);
    }
    let global = loop_flags.contains('g');
    if loop_flags.contains('w') {
        return if global {
            Some(OptionRowsKey::GlobalWindow)
        } else {
            parse_window(&values.window_id).map(OptionRowsKey::Window)
        };
    }
    if loop_flags.contains('s') {
        return if global {
            Some(OptionRowsKey::GlobalSession)
        } else {
            parse_session(&values.session_id).map(OptionRowsKey::Session)
        };
    }
    if loop_flags.contains('p') {
        return if global {
            None
        } else {
            parse_pane(&values.pane_id).map(OptionRowsKey::Pane)
        };
    }
    global.then_some(OptionRowsKey::GlobalSession)
}

pub(super) fn environment_loop_variables(
    row: &FormatEnvironRow,
    index: usize,
    last: bool,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[0].to_owned(),
            bool_string(row.hidden).to_owned(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[1].to_owned(),
            row.name.clone(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[2].to_owned(),
            bool_string(row.removed).to_owned(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[3].to_owned(),
            row.value.to_string(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[4].to_owned(),
            index.to_string(),
        ),
        (
            ENVIRONMENT_LOOP_CONTEXT_FORMATS[5].to_owned(),
            bool_string(last).to_owned(),
        ),
    ])
}

pub(super) fn option_loop_variables(
    row: &FormatOptionRow,
    index: usize,
    last: bool,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (OPTION_LOOP_CONTEXT_FORMATS[0].to_owned(), index.to_string()),
        (
            OPTION_LOOP_CONTEXT_FORMATS[1].to_owned(),
            bool_string(last).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[2].to_owned(),
            row.array_count.to_string(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[3].to_owned(),
            bool_string(row.array_first).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[4].to_owned(),
            row.array_index.clone(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[5].to_owned(),
            row.array_key.clone(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[6].to_owned(),
            bool_string(row.array_last).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[7].to_owned(),
            bool_string(row.is_array).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[8].to_owned(),
            bool_string(row.is_hook).to_owned(),
        ),
        (
            OPTION_LOOP_CONTEXT_FORMATS[9].to_owned(),
            bool_string(row.is_user).to_owned(),
        ),
        (OPTION_LOOP_CONTEXT_FORMATS[10].to_owned(), row.name.clone()),
        (
            OPTION_LOOP_CONTEXT_FORMATS[11].to_owned(),
            row.value.clone(),
        ),
    ])
}
