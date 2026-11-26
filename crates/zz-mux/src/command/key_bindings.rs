use super::*;

impl MuxEngine {
    pub(super) fn bind_key(
        &mut self,
        command: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &command.args;
        let spec = command_spec("bind-key").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, command)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("bind-key", args)?;
        let table = key_table(&options);
        let repeat = options.has("-r");
        let note = options.value("-N").map(str::to_owned);
        let key = required_arg(&positional, 0, "key")?;
        let key = if command.argument_is_command_block(positional_start) {
            normalize_typed_command_block(
                self,
                crate::parser::command_block_body(key).unwrap_or(key),
            )?
        } else {
            key.to_owned()
        };
        let key = parse_tmux_key(&key)
            .filter(|key| key != "None")
            .ok_or_else(|| ServerError::InvalidCommand(format!("unknown key: {key}")))?;
        if positional.len() == 1 {
            self.keys.update_binding_metadata(table, &key, note, repeat);
            return Ok(Execution::default());
        }
        let tail_start = positional_start + 1;
        let commands = bound_commands(self, command, tail_start)?;
        self.keys.bind(
            table,
            &key,
            Binding {
                commands,
                repeat,
                note,
            },
        );
        Ok(Execution::default())
    }

    pub(super) fn unbind_key(&mut self, args: &[RawText]) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("unbind-key", args)?;
        let table = key_table(&options);
        let quiet = options.has("-q");
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "command unbind-key: too many arguments (need at most 1)".to_owned(),
            ));
        }
        if options.has("-a") {
            if !positional.is_empty() {
                return quiet_command_error(quiet, "key given with -a");
            }
            if table != "root" && self.keys.table_names().all(|name| name != table) {
                return quiet_command_error(quiet, format!("table {table} doesn't exist"));
            }
            self.keys.remove_table(table);
            return Ok(Execution::default());
        }
        let Some(key) = positional.first() else {
            return quiet_command_error(quiet, "missing key");
        };
        let Some(canonical) = parse_tmux_key(key).filter(|key| key != "None") else {
            return quiet_command_error(quiet, format!("unknown key: {key}"));
        };
        if options.value("-T").is_some()
            && table != "root"
            && self.keys.table_names().all(|name| name != table)
        {
            return quiet_command_error(quiet, format!("table {table} doesn't exist"));
        }
        self.keys.unbind(table, &canonical);
        Ok(Execution::default())
    }

    pub(super) fn list_keys(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let mut listing = self.start_key_listing(context, args, hooks)?;
        loop {
            if let Some(execution) = self.step_key_listing(&mut listing, hooks) {
                return Ok(execution);
            }
        }
    }

    pub fn start_key_listing(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<KeyListing, ServerError> {
        let (options, positional) = parse_command_options("list-keys", args)?;
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "command list-keys: too many arguments (need at most 1)".to_owned(),
            ));
        }
        let filter_key = positional
            .first()
            .map(|key| {
                parse_tmux_key_details(key)
                    .ok_or_else(|| ServerError::InvalidCommand(format!("invalid key: {key}")))
            })
            .transpose()?;
        let sort = TmuxSort::parse(options.value("-O"), options.has("-r"), None)?;
        if let Some(table) = options.value("-T")
            && table != "root"
            && self.keys.table_names().all(|name| name != table)
        {
            return Err(ServerError::InvalidCommand(format!(
                "table {table} doesn't exist"
            )));
        }
        let notes_only = options.has("-N");
        let format = options.value("-F").unwrap_or(DEFAULT_LIST_KEYS_FORMAT);
        let cacheable = format == DEFAULT_LIST_KEYS_FORMAT
            && hooks.stable_option_lookups()
            && ((hooks.only_tmux_options() && *LIST_KEY_FORMAT_NAMES_ARE_NOT_OPTIONS) || {
                let values = self.format_status_context_with_format_client(
                    context.session,
                    context.window,
                    context.pane,
                    context.session,
                    context.target_format_client(),
                );
                !LIST_KEY_BINDING_CONTEXT_FORMATS
                    .iter()
                    .chain(LIST_KEY_SUMMARY_CONTEXT_FORMATS)
                    .any(|name| hooks.option_variable(name, &values).is_some())
            });
        if cacheable && let Some((output, had_binding)) = self.cached_key_listing(args) {
            return Ok(KeyListing {
                values: StatusContext::default(),
                options: Arc::default(),
                user_options: FormatFacts::default(),
                fallback: StatusContext::default(),
                context: context.clone(),
                args: Vec::new(),
                generation: self.keys.generation(),
                cacheable: false,
                single: options.has("-1"),
                had_binding,
                format: String::new(),
                bindings: Vec::new(),
                next: 0,
                prefix: String::new(),
                notes_only,
                has_repeat: false,
                key_width: String::new(),
                table_width: String::new(),
                output,
            });
        }
        let mut bindings;
        if let Some(table) = options.value("-T") {
            bindings = listed_keys(self.keys.list(Some(table)));
            sort_listed_keys(&mut bindings, sort);
        } else if notes_only {
            bindings = listed_keys(self.keys.list(Some("prefix")));
            sort_listed_keys(&mut bindings, sort);
            let mut root = listed_keys(self.keys.list(Some("root")));
            sort_listed_keys(&mut root, sort);
            bindings.extend(root);
        } else {
            bindings = listed_keys(self.keys.list(None));
            sort_listed_keys(&mut bindings, sort);
        }
        if notes_only && !options.has("-a") {
            bindings.retain(|binding| binding.binding.note.is_some());
        }
        if let Some(filter_key) = &filter_key {
            bindings.retain(|binding| binding.identity.masked_eq(&filter_key.identity));
            if bindings.is_empty() {
                return Err(ServerError::InvalidCommand(format!(
                    "unknown key: {}",
                    positional[0]
                )));
            }
        }
        if options.has("-1") {
            bindings.truncate(1);
        }
        let has_repeat = bindings.iter().any(|binding| binding.binding.repeat);
        let key_width = bindings
            .iter()
            .map(|binding| crate::display_width(binding.key.as_str()))
            .max()
            .unwrap_or_default();
        let table_width = bindings
            .iter()
            .map(|binding| crate::display_width(binding.table))
            .max()
            .unwrap_or_default();
        let key_width = key_width.to_string();
        let table_width = table_width.to_string();
        let prefix = options.value("-P").map_or_else(
            || {
                let prefix = self.keys.prefix();
                if prefix.eq_ignore_ascii_case("none") {
                    String::new()
                } else {
                    tmux_key_display(prefix)
                }
            },
            str::to_owned,
        );
        let had_binding = !bindings.is_empty();
        let prepared = PreparedFormat::new(
            self,
            FormatContext {
                session: context.session,
                window: context.window,
                pane: context.pane,
                active_session: context.session,
                format_client: context.target_format_client(),
                format_type: FormatType::None,
            },
        );
        let values = prepared
            .values()
            .clone()
            .detach_with_templates(self.format_needs([format]), [format]);
        let fallback = match context.target_format_client() {
            FormatClient::Attached(session) => Some(session),
            _ => None,
        }
        .map(|session| {
            let window = self
                .state
                .sessions
                .get(&session)
                .map(|session| session.active_window);
            let pane = window.and_then(|window| {
                self.state
                    .windows
                    .get(&window)
                    .map(|window| window.active_pane)
            });
            self.format_status_context_with_format_client(
                Some(session),
                window,
                pane,
                Some(session),
                context.target_format_client(),
            )
            .detach(crate::formats::FormatNeeds::NONE)
        })
        .unwrap_or_default();
        Ok(KeyListing {
            values,
            options: self.cached_format_option_snapshot(),
            user_options: self.format_facts(),
            fallback,
            context: context.clone(),
            args: args.to_vec(),
            generation: self.keys.generation(),
            cacheable,
            single: options.has("-1"),
            had_binding,
            format: format.to_owned(),
            bindings: bindings
                .into_iter()
                .map(|listed| OwnedListedKey {
                    table: listed.table.to_owned(),
                    key: listed.key,
                    binding: listed.binding.clone(),
                })
                .collect(),
            next: 0,
            prefix,
            notes_only,
            has_repeat,
            key_width,
            table_width,
            output: String::new(),
        })
    }

    pub fn step_key_listing(
        &self,
        listing: &mut KeyListing,
        hooks: &mut impl StatusHooks,
    ) -> Option<Execution> {
        if listing.next < listing.bindings.len() {
            let end = (listing.next + 64).min(listing.bindings.len());
            for listed in &listing.bindings[listing.next..end] {
                let mut item_hooks = ListKeyHooks {
                    inner: &mut *hooks,
                    table: &listed.table,
                    key: &listed.key,
                    binding: &listed.binding,
                    prefix: &listing.prefix,
                    notes_only: listing.notes_only,
                    has_repeat: listing.has_repeat,
                    key_width: &listing.key_width,
                    table_width: &listing.table_width,
                    options: &listing.options,
                    user_options: &listing.user_options,
                    fallback: &listing.fallback,
                };
                let line = crate::formats::expand_format_bytes(
                    &listing.format,
                    &listing.values,
                    &mut item_hooks,
                );
                if !line.is_empty() {
                    if !listing.output.is_empty() {
                        listing.output.push('\n');
                    }
                    listing.output.push_str(&line);
                }
            }
            listing.next = end;
            if end < listing.bindings.len() {
                return None;
            }
        }
        let output = std::mem::take(&mut listing.output);
        if listing.cacheable && listing.generation == self.keys.generation() {
            let bytes = std::mem::size_of::<CachedKeyListing>()
                + output.len()
                + listing
                    .args
                    .iter()
                    .map(|argument| {
                        std::mem::size_of::<RawText>() + argument.len() + argument.as_bytes().len()
                    })
                    .sum::<usize>();
            *self.key_listing_cache.lock() = (bytes <= 1024 * 1024).then(|| CachedKeyListing {
                generation: listing.generation,
                args: listing.args.clone(),
                output: output.clone(),
                had_binding: listing.had_binding,
            });
        }
        Some(self.key_listing_execution(
            &listing.context,
            listing.single,
            listing.had_binding,
            output,
        ))
    }

    fn cached_key_listing(&self, args: &[RawText]) -> Option<(String, bool)> {
        let cache = self.key_listing_cache.lock();
        let cached = cache.as_ref()?;
        (cached.generation == self.keys.generation() && cached.args == args)
            .then(|| (cached.output.clone(), cached.had_binding))
    }

    fn key_listing_execution(
        &self,
        context: &ExecutionContext,
        single: bool,
        had_binding: bool,
        output: String,
    ) -> Execution {
        if single && had_binding {
            let duration_ms = context
                .session
                .map_or(self.global_display_time_ms, |session| {
                    self.display_time_for_session(session)
                });
            Execution::effect(MuxEffect::PrintOrMessage {
                pane: context.pane,
                text: output,
                duration_ms,
                freeze: true,
            })
        } else {
            Execution::output(output)
        }
    }

    pub(super) fn list_commands(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("list-commands", args)?;
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "list-commands accepts at most one command".to_owned(),
            ));
        }
        let listed_spec = |name: &str| -> Result<&'static CommandSpec, ServerError> {
            let name = match resolve_command(name) {
                CommandResolution::Canonical(name) => name,
                CommandResolution::Unimplemented(name) => {
                    return Err(ServerError::InvalidCommand(format!(
                        "unknown command: {name}"
                    )));
                }
                CommandResolution::Ambiguous(message) => {
                    return Err(ServerError::InvalidCommand(message));
                }
                CommandResolution::Unknown => {
                    return Err(ServerError::InvalidCommand(format!(
                        "unknown command: {name}"
                    )));
                }
            };
            catalog_command_spec(name)
                .ok_or_else(|| ServerError::InvalidCommand(format!("unknown command: {name}")))
        };
        let mut specs = if let Some(name) = positional.first() {
            vec![listed_spec(name)?]
        } else {
            command_specs().collect::<Vec<_>>()
        };
        specs.sort_by_key(|spec| spec.name);
        let format = options.value("-F").unwrap_or(DEFAULT_LIST_COMMANDS_FORMAT);
        let mut output = Vec::with_capacity(specs.len());
        let prepared = PreparedFormat::new(
            self,
            FormatContext {
                session: context.session,
                window: context.window,
                pane: context.pane,
                active_session: context.session,
                format_client: FormatClient::NoClient,
                format_type: FormatType::None,
            },
        );
        for spec in specs {
            let mut item_hooks = ListCommandHooks {
                inner: &mut *hooks,
                spec,
            };
            output.push(prepared.expand(format, &mut item_hooks));
        }
        Ok(Execution::output(RawText::join(&output, b"\n")))
    }
}

pub(super) fn listed_keys<'a>(
    bindings: impl Iterator<Item = (&'a str, &'a str, &'a Binding)>,
) -> Vec<ListedKey<'a>> {
    let mut listed = bindings
        .enumerate()
        .map(|(original, (table, key, binding))| {
            let (key, identity) = parse_tmux_key_details(key).map_or_else(
                || {
                    (
                        key.to_owned(),
                        TmuxKeyIdentity {
                            base: u32::MAX,
                            key_type: u8::MAX,
                            modifiers: u8::MAX,
                            flags: u8::MAX,
                            canonical: key.to_owned(),
                        },
                    )
                },
                |parsed| (parsed.rendered, parsed.identity),
            );
            ListedKey {
                table,
                key,
                binding,
                identity,
                original,
            }
        })
        .collect::<Vec<_>>();
    listed.sort_by(|left, right| {
        left.table
            .cmp(right.table)
            .then_with(|| left.identity.modifiers.cmp(&right.identity.modifiers))
            .then_with(|| left.identity.key_type.cmp(&right.identity.key_type))
            .then_with(|| left.identity.base.cmp(&right.identity.base))
            .then_with(|| left.identity.canonical.cmp(&right.identity.canonical))
    });
    for (original, binding) in listed.iter_mut().enumerate() {
        binding.original = original;
    }
    listed
}

fn sort_listed_keys(bindings: &mut [ListedKey<'_>], sort: TmuxSort) {
    let order = sort.order();
    sort.apply(bindings, |left, right| {
        let primary = match order {
            Some(TmuxSortOrder::Index) => left.identity.cmp(&right.identity),
            Some(TmuxSortOrder::Modifier) => left
                .identity
                .modifiers
                .cmp(&right.identity.modifiers)
                .then_with(|| left.identity.base.cmp(&right.identity.base))
                .then_with(|| left.identity.key_type.cmp(&right.identity.key_type))
                .then_with(|| left.identity.flags.cmp(&right.identity.flags))
                .then_with(|| left.identity.canonical.cmp(&right.identity.canonical)),
            Some(TmuxSortOrder::Name) => left
                .table
                .to_ascii_lowercase()
                .cmp(&right.table.to_ascii_lowercase())
                .then_with(|| left.table.cmp(right.table)),
            Some(
                TmuxSortOrder::Activity
                | TmuxSortOrder::Creation
                | TmuxSortOrder::Order
                | TmuxSortOrder::Size
                | TmuxSortOrder::Z,
            )
            | None => Ordering::Equal,
        };
        primary
            .then_with(|| left.table.cmp(right.table))
            .then_with(|| left.original.cmp(&right.original))
    });
}

pub(super) fn parse_tmux_key_details(value: &str) -> Option<ParsedTmuxKey> {
    let canonical = parse_tmux_key(value)?;
    let rendered = tmux_key_display(&canonical);
    let mut identity = tmux_key_identity(&canonical)?;
    identity.canonical.clone_from(&rendered);
    Some(ParsedTmuxKey { rendered, identity })
}

pub(super) fn tmux_key_display(canonical: &str) -> String {
    if let Some(code) = tmux_hex_ascii_code(canonical) {
        return char::from(code).to_string();
    }
    if let Some(modifiers) = canonical.strip_suffix('\x7f') {
        return format!("{modifiers}C-?");
    }
    canonical.strip_suffix(' ').map_or_else(
        || canonical.to_owned(),
        |modifiers| format!("{modifiers}Space"),
    )
}

pub(super) fn tmux_key_identity(canonical: &str) -> Option<TmuxKeyIdentity> {
    let mut control = false;
    let mut meta = false;
    let mut shift = false;
    let mut base = canonical;
    loop {
        if let Some(unmodified) = base.strip_prefix("C-") {
            control = true;
            base = unmodified;
        } else if let Some(unmodified) = base.strip_prefix("M-") {
            meta = true;
            base = unmodified;
        } else if let Some(unmodified) = base.strip_prefix("S-") {
            shift = true;
            base = unmodified;
        } else {
            break;
        }
    }
    let modifiers = u8::from(meta) | (u8::from(control) << 1) | (u8::from(shift) << 2);
    let (base_code, key_type, flags) = tmux_key_base_identity(base)?;
    Some(TmuxKeyIdentity {
        base: base_code,
        key_type,
        modifiers,
        flags,
        canonical: canonical.to_owned(),
    })
}

fn tmux_key_base_identity(base: &str) -> Option<(u32, u8, u8)> {
    if let Some(code) = tmux_hex_ascii_code(base) {
        let packed = u32::from(code) | (1 << 24) | (2 << 29);
        return Some((packed, 0, 0));
    }
    let mut characters = base.chars();
    if let (Some(character), None) = (characters.next(), characters.next()) {
        if character.is_ascii() {
            return Some((u32::from(character), 0, 0));
        }
        let bytes = base.as_bytes();
        if bytes.len() <= 3 {
            let packed = bytes
                .iter()
                .enumerate()
                .fold(0_u32, |value, (index, byte)| {
                    value | (u32::from(*byte) << (index * 8))
                });
            let width = u32::try_from(crate::display_width(base)).ok()?.min(2);
            let packed = packed | (u32::try_from(bytes.len()).ok()? << 24) | ((width + 1) << 29);
            return Some((packed, 0, 0));
        }
        return Some((u32::from(character), 0, 0));
    }
    if base == "None" {
        return Some((0, 2, 0));
    }
    if base == "Any" {
        return Some((4, 2, 0));
    }
    if let Some(index) = TMUX_CONTROL_KEY_NAMES.iter().position(|key| *key == base) {
        return Some((u32::try_from(index).ok()?, 0, 0));
    }
    if base == "Space" {
        return Some((32, 0, 0));
    }
    if let Some(number) = base
        .strip_prefix('F')
        .and_then(|value| value.parse::<u32>().ok())
        && (1..=12).contains(&number)
    {
        return Some((number + 7, 2, 8));
    }
    const SPECIAL_KEYS: &[(&str, u32, u8)] = &[
        ("BSpace", 7, 0),
        ("IC", 20, 8),
        ("DC", 21, 8),
        ("Home", 22, 8),
        ("End", 23, 8),
        ("NPage", 24, 8),
        ("PPage", 25, 8),
        ("BTab", 26, 0),
        ("Up", 27, 12),
        ("Down", 28, 12),
        ("Left", 29, 12),
        ("Right", 30, 12),
        ("KP/", 31, 2),
        ("KP*", 32, 2),
        ("KP-", 33, 2),
        ("KP7", 34, 2),
        ("KP8", 35, 2),
        ("KP9", 36, 2),
        ("KP+", 37, 2),
        ("KP4", 38, 2),
        ("KP5", 39, 2),
        ("KP6", 40, 2),
        ("KP1", 41, 2),
        ("KP2", 42, 2),
        ("KP3", 43, 2),
        ("KPEnter", 44, 2),
        ("KP0", 45, 2),
        ("KP.", 46, 2),
    ];
    if let Some((_, base, flags)) = SPECIAL_KEYS.iter().find(|(name, _, _)| *name == base) {
        return Some((*base, 2, *flags));
    }
    if let Some(number) = base
        .strip_prefix("User")
        .and_then(|value| value.parse::<u32>().ok())
    {
        return Some((number, 1, 0));
    }
    mouse_key_identity(base)
}

#[must_use]
pub fn mouse_key_is_a_replayed_double_click(key: &str) -> bool {
    key.rsplit_once('-')
        .map_or(key, |(_, base)| base)
        .starts_with("DoubleClick")
}

fn mouse_key_identity(base: &str) -> Option<(u32, u8, u8)> {
    const LOCATIONS: &[(&str, u32)] = &[
        ("StatusDefault", 4),
        ("ScrollbarSlider", 7),
        ("ScrollbarDown", 8),
        ("ScrollbarUp", 6),
        ("StatusRight", 3),
        ("StatusLeft", 2),
        ("Status", 1),
        ("Border", 5),
        ("Pane", 0),
        ("Empty", 9),
        ("Control0", 10),
        ("Control1", 11),
        ("Control2", 12),
        ("Control3", 13),
        ("Control4", 14),
        ("Control5", 15),
        ("Control6", 16),
        ("Control7", 17),
        ("Control8", 18),
        ("Control9", 19),
    ];
    const EVENTS: &[(&str, u8, bool)] = &[
        ("MouseDragEnd", 7, true),
        ("MouseDrag", 6, true),
        ("MouseDown", 4, true),
        ("MouseUp", 5, true),
        ("SecondClick", 10, true),
        ("DoubleClick", 11, true),
        ("TripleClick", 12, true),
        ("WheelDown", 8, false),
        ("WheelUp", 9, false),
    ];
    for (event, key_type, has_button) in EVENTS {
        let Some(tail) = base.strip_prefix(event) else {
            continue;
        };
        let (button, location) = if *has_button {
            const BUTTONS: &[&str] = &["10", "11", "1", "2", "3", "6", "7", "8", "9"];
            let button = BUTTONS.iter().find(|button| tail.starts_with(**button))?;
            (button.parse::<u32>().ok()?, &tail[button.len()..])
        } else {
            (0, tail)
        };
        let location = LOCATIONS
            .iter()
            .find_map(|(name, rank)| (*name == location).then_some(*rank))?;
        return Some(((button << 8) | location, *key_type, 0));
    }
    None
}

pub fn parse_tmux_key(value: &str) -> Option<String> {
    if value.eq_ignore_ascii_case("none") {
        return Some("None".to_owned());
    }
    if value.eq_ignore_ascii_case("any") {
        return Some("Any".to_owned());
    }
    let bytes = value.as_bytes();
    if bytes.len() == 2 && bytes[0] == b'^' && (1..32).contains(&bytes[1]) {
        return Some(format!(
            "C-{}",
            TMUX_CONTROL_KEY_NAMES[usize::from(bytes[1])]
        ));
    }
    if let Some(hex) = value.strip_prefix("0x") {
        let code = parse_scanf_unsigned_prefix(hex, 16)?;
        if code < 32 {
            return Some(TMUX_CONTROL_KEY_NAMES[code as usize].to_owned());
        }
        if code <= 127 {
            return Some(format!("0x{code:x}"));
        }
        return char::from_u32(code).map(|character| character.to_string());
    }

    let mut control = false;
    let mut meta = false;
    let mut shift = false;
    let short_control = value
        .strip_prefix('^')
        .filter(|tail| !tail.is_empty() && value.len() == 2)
        .map(str::to_ascii_lowercase);
    let mut base = value;
    if let Some(unmodified) = base.strip_prefix('^').filter(|tail| !tail.is_empty()) {
        control = true;
        base = short_control.as_deref().unwrap_or(unmodified);
    }
    loop {
        if let Some(unmodified) = strip_prefix_ascii_case(base, "C-") {
            control = true;
            base = unmodified;
        } else if let Some(unmodified) = strip_prefix_ascii_case(base, "M-") {
            meta = true;
            base = unmodified;
        } else if let Some(unmodified) = strip_prefix_ascii_case(base, "S-") {
            shift = true;
            base = unmodified;
        } else {
            break;
        }
    }
    if base.is_empty() {
        return None;
    }

    let mut characters = base.chars();
    let canonical = if let (Some(character), None) = (characters.next(), characters.next()) {
        if u32::from(character) < 32 {
            return None;
        }
        character.to_string()
    } else if let Some(named) = canonical_named_key(base) {
        named.to_owned()
    } else if let Some(number) = base.get(1..).filter(|_| {
        base.as_bytes()
            .first()
            .is_some_and(|first| first.eq_ignore_ascii_case(&b'F'))
    }) {
        if !matches!(
            number,
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "10" | "11" | "12"
        ) {
            return None;
        }
        format!("F{number}")
    } else if let Some(user) = parse_user_key(base) {
        user
    } else {
        parse_mouse_key(base)?
    };

    let mut parsed = String::new();
    if control {
        parsed.push_str("C-");
    }
    if meta {
        parsed.push_str("M-");
    }
    if shift {
        parsed.push_str("S-");
    }
    parsed.push_str(&canonical);
    Some(parsed)
}

pub(super) const TMUX_CONTROL_KEY_NAMES: [&str; 32] = [
    "[NUL]", "[SOH]", "[STX]", "[ETX]", "[EOT]", "[ENQ]", "[ASC]", "[BEL]", "[BS]", "Tab", "[LF]",
    "[VT]", "[FF]", "Enter", "[SO]", "[SI]", "[DLE]", "[DC1]", "[DC2]", "[DC3]", "[DC4]", "[NAK]",
    "[SYN]", "[ETB]", "[CAN]", "[EM]", "[SUB]", "Escape", "[FS]", "[GS]", "[RS]", "[US]",
];

pub(super) fn tmux_hex_ascii_code(value: &str) -> Option<u8> {
    let code = parse_scanf_unsigned_prefix(value.strip_prefix("0x")?, 16)?;
    u8::try_from(code)
        .ok()
        .filter(|code| (32..=127).contains(code))
}

pub(super) fn tmux_option_key_display(name: &str, value: &str) -> String {
    if name == "prefix2" {
        tmux_key_display(value)
    } else {
        value.to_owned()
    }
}

fn strip_prefix_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .and_then(|_| value.get(prefix.len()..))
}

pub(super) fn canonical_named_key(value: &str) -> Option<&'static str> {
    const NAMES: &[(&str, &str)] = &[
        ("IC", "IC"),
        ("Insert", "IC"),
        ("DC", "DC"),
        ("Delete", "DC"),
        ("Home", "Home"),
        ("End", "End"),
        ("NPage", "NPage"),
        ("PageDown", "NPage"),
        ("PgDn", "NPage"),
        ("PPage", "PPage"),
        ("PageUp", "PPage"),
        ("PgUp", "PPage"),
        ("BTab", "BTab"),
        ("Space", "Space"),
        ("BSpace", "BSpace"),
        ("[NUL]", "[NUL]"),
        ("[SOH]", "[SOH]"),
        ("[STX]", "[STX]"),
        ("[ETX]", "[ETX]"),
        ("[EOT]", "[EOT]"),
        ("[ENQ]", "[ENQ]"),
        ("[ASC]", "[ASC]"),
        ("[BEL]", "[BEL]"),
        ("[BS]", "[BS]"),
        ("Tab", "Tab"),
        ("[LF]", "[LF]"),
        ("[VT]", "[VT]"),
        ("[FF]", "[FF]"),
        ("Enter", "Enter"),
        ("[SO]", "[SO]"),
        ("[SI]", "[SI]"),
        ("[DLE]", "[DLE]"),
        ("[DC1]", "[DC1]"),
        ("[DC2]", "[DC2]"),
        ("[DC3]", "[DC3]"),
        ("[DC4]", "[DC4]"),
        ("[NAK]", "[NAK]"),
        ("[SYN]", "[SYN]"),
        ("[ETB]", "[ETB]"),
        ("[CAN]", "[CAN]"),
        ("[EM]", "[EM]"),
        ("[SUB]", "[SUB]"),
        ("Escape", "Escape"),
        ("[FS]", "[FS]"),
        ("[GS]", "[GS]"),
        ("[RS]", "[RS]"),
        ("[US]", "[US]"),
        ("Up", "Up"),
        ("Down", "Down"),
        ("Left", "Left"),
        ("Right", "Right"),
        ("KP/", "KP/"),
        ("KP*", "KP*"),
        ("KP-", "KP-"),
        ("KP7", "KP7"),
        ("KP8", "KP8"),
        ("KP9", "KP9"),
        ("KP+", "KP+"),
        ("KP4", "KP4"),
        ("KP5", "KP5"),
        ("KP6", "KP6"),
        ("KP1", "KP1"),
        ("KP2", "KP2"),
        ("KP3", "KP3"),
        ("KPEnter", "KPEnter"),
        ("KP0", "KP0"),
        ("KP.", "KP."),
    ];
    NAMES
        .iter()
        .find_map(|(name, canonical)| value.eq_ignore_ascii_case(name).then_some(*canonical))
}

pub(super) const MAX_USER_KEY: usize = 1000;

fn parse_user_key(value: &str) -> Option<String> {
    let tail = value.strip_prefix("User")?;
    let number = parse_scanf_unsigned_prefix(tail, 10)?;
    (number <= 1000).then(|| format!("User{number}"))
}

fn parse_scanf_unsigned_prefix(value: &str, radix: u32) -> Option<u32> {
    let whitespace = value.bytes().take_while(u8::is_ascii_whitespace).count();
    let mut value = value.get(whitespace..)?;
    let negative = if let Some(tail) = value.strip_prefix('-') {
        value = tail;
        true
    } else {
        if let Some(tail) = value.strip_prefix('+') {
            value = tail;
        }
        false
    };
    if radix == 16
        && let Some(tail) = value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
        && tail.as_bytes().first().is_some_and(u8::is_ascii_hexdigit)
    {
        value = tail;
    }
    let digits = value
        .bytes()
        .take_while(|byte| match radix {
            10 => byte.is_ascii_digit(),
            16 => byte.is_ascii_hexdigit(),
            _ => false,
        })
        .count();
    let magnitude = u64::from_str_radix(value.get(..digits)?, radix).ok()?;
    let number = if negative {
        0_u64.wrapping_sub(magnitude)
    } else {
        magnitude
    };
    Some(number as u32)
}

fn parse_mouse_key(value: &str) -> Option<String> {
    const LOCATIONS: &[&str] = &[
        "Pane",
        "Status",
        "StatusLeft",
        "StatusRight",
        "StatusDefault",
        "ScrollbarUp",
        "ScrollbarSlider",
        "ScrollbarDown",
        "Border",
        "Empty",
        "Control0",
        "Control1",
        "Control2",
        "Control3",
        "Control4",
        "Control5",
        "Control6",
        "Control7",
        "Control8",
        "Control9",
    ];
    const BUTTONS: &[&str] = &["10", "11", "1", "2", "3", "6", "7", "8", "9"];
    const EVENTS: &[&str] = &[
        "MouseDown",
        "MouseUp",
        "MouseDragEnd",
        "MouseDrag",
        "SecondClick",
        "DoubleClick",
        "TripleClick",
    ];
    for event in ["WheelUp", "WheelDown"] {
        if let Some(location) = strip_prefix_ascii_case(value, event)
            && let Some(location) = LOCATIONS
                .iter()
                .find(|candidate| location.eq_ignore_ascii_case(candidate))
        {
            return Some(format!("{event}{location}"));
        }
    }
    for event in EVENTS {
        let Some(tail) = strip_prefix_ascii_case(value, event) else {
            continue;
        };
        for button in BUTTONS {
            let Some(location) = tail.strip_prefix(button) else {
                continue;
            };
            if let Some(location) = LOCATIONS
                .iter()
                .find(|candidate| location.eq_ignore_ascii_case(candidate))
            {
                return Some(format!("{event}{button}{location}"));
            }
        }
    }
    None
}

pub(super) fn key_table(options: &Options) -> &str {
    options
        .value("-T")
        .unwrap_or(if options.has("-n") { "root" } else { "prefix" })
}
