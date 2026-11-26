use super::*;

impl<V: FormatVariables + ?Sized, H: StatusHooks> Expander<'_, V, H> {
    const fn tracing(&self) -> bool {
        self.trace.is_some()
    }

    fn log(&mut self, depth: usize, message: &str) {
        if let Some(trace) = self.trace.as_deref_mut() {
            let indent = depth.min(FORMAT_LOOP_LIMIT);
            trace.push(format!("#{:indent$}{message}", ""));
        }
    }

    fn expand_cycle(&mut self, frames: &str, count: u64) -> String {
        if frames.is_empty() {
            return String::new();
        }
        let Some(clock) = self.hooks.cycle_clock() else {
            return String::new();
        };
        let total = frames.bytes().filter(|byte| *byte == b',').count() + 1;
        let index =
            usize::try_from(clock / (count * FORMAT_CYCLE_PERIOD_MS)).unwrap_or(usize::MAX) % total;
        if total > 1 {
            self.hooks.cycle_animates();
        }
        frames.split(',').nth(index).unwrap_or_default().to_owned()
    }

    pub(super) fn expand(&mut self, format: &str, depth: usize) -> RawText {
        if self.tracing() || !compiled::enabled() {
            return self.expand_interpreted(format, depth);
        }
        if format.is_empty() || depth == FORMAT_LOOP_LIMIT {
            return RawText::default();
        }
        let expanded;
        let format = if self.time && format.contains('%') {
            expanded = self.hooks.strftime(format);
            expanded.as_str()
        } else {
            format
        };
        if plain_format(format, false) {
            return RawText::from(format);
        }
        let template = compiled::get(format);
        let mut output = Vec::with_capacity(format.len());
        self.expand_compiled_ops(&template, depth, &mut output, false);
        RawText::from_bytes(output)
    }

    fn expand_compiled_ops(
        &mut self,
        template: &compiled::Template,
        depth: usize,
        output: &mut Vec<u8>,
        style: bool,
    ) {
        for operation in &template.operations {
            match operation {
                compiled::Operation::Text(text) => output.extend_from_slice(text.as_bytes()),
                compiled::Operation::Variable(name) => {
                    output.extend_from_slice(
                        self.context.variable(name).unwrap_or_default().as_bytes(),
                    );
                }
                compiled::Operation::Shell(command) => {
                    output.extend_from_slice(self.hooks.shell(command, &self.job_tag).as_bytes());
                }
                compiled::Operation::Replacement(replacement) => {
                    if replacement.direct && !style {
                        self.append_lookup(&replacement.copy, output, 0);
                        continue;
                    }
                    let value = if depth + 1 == FORMAT_LOOP_LIMIT {
                        self.expand_replacement_inner(&replacement.body, depth + 1)
                    } else if let Some(arguments) = &replacement.dynamic_padding {
                        let mut flags = replacement
                            .flags
                            .as_ref()
                            .unwrap()
                            .view(&replacement.modifiers);
                        for (modifier, last) in arguments {
                            let width =
                                self.expand(&replacement.modifiers[*modifier].args[0], depth + 1);
                            if *last {
                                flags.padding = width
                                    .parse::<isize>()
                                    .ok()
                                    .filter(|width| {
                                        (-FORMAT_MAX_WIDTH..=FORMAT_MAX_WIDTH).contains(width)
                                    })
                                    .unwrap_or_default();
                            }
                        }
                        if replacement.padding_direct && !style {
                            self.append_lookup(&replacement.copy, output, flags.padding);
                            continue;
                        }
                        self.expand_replacement_value(&replacement.copy, depth + 1, &flags)
                    } else {
                        for argument in &replacement.discarded {
                            self.expand(argument, depth + 1);
                        }
                        let expanded;
                        let modifiers = if replacement.dynamic {
                            expanded = replacement
                                .modifiers
                                .iter()
                                .map(|modifier| Modifier {
                                    kind: modifier.kind,
                                    text: modifier.text,
                                    args: modifier
                                        .args
                                        .iter()
                                        .map(|argument| {
                                            self.expand(argument, depth + 1).to_string()
                                        })
                                        .collect(),
                                })
                                .collect::<Vec<_>>();
                            expanded.as_slice()
                        } else {
                            &replacement.modifiers
                        };
                        let flags = replacement.flags.as_ref().map_or_else(
                            || ModifierFlags::from_modifiers(modifiers),
                            |flags| flags.view(modifiers),
                        );
                        if replacement.padding_direct && !style {
                            self.append_lookup(&replacement.copy, output, flags.padding);
                            continue;
                        }
                        self.expand_replacement_value(&replacement.copy, depth + 1, &flags)
                    };
                    match value {
                        Ok(value) => output.extend_from_slice(if style {
                            value.as_str().as_bytes()
                        } else {
                            value.as_bytes()
                        }),
                        Err(()) => break,
                    }
                }
                compiled::Operation::Style(prefix, style) => {
                    output.extend_from_slice(prefix.as_bytes());
                    self.expand_compiled_ops(style, depth, output, true);
                    output.push(b']');
                }
            }
        }
    }

    fn expand_interpreted(&mut self, format: &str, depth: usize) -> RawText {
        if format.is_empty() {
            return RawText::default();
        }
        if depth == FORMAT_LOOP_LIMIT {
            if self.tracing() {
                self.log(depth, &format!("reached loop limit ({FORMAT_LOOP_LIMIT})"));
            }
            return RawText::default();
        }
        if self.tracing() {
            self.log(depth + 1, &format!("expanding format: {format}"));
        }
        let time_expanded;
        let format = if self.time && format.contains('%') {
            time_expanded = self.hooks.strftime(format);
            if self.tracing() && time_expanded != format {
                self.log(depth + 1, &format!("after time expanded: {time_expanded}"));
            }
            time_expanded.as_str()
        } else {
            format
        };
        let mut output = Vec::with_capacity(format.len());
        let mut literal = String::new();
        let mut index = 0usize;
        while index < format.len() {
            let Some(character) = format[index..].chars().next() else {
                break;
            };
            if character != '#' {
                literal.push(character);
                index += character.len_utf8();
                continue;
            }
            let hashes_end = format.as_bytes()[index..]
                .iter()
                .position(|byte| *byte != b'#')
                .map_or(format.len(), |offset| index + offset);
            if format.as_bytes().get(hashes_end) == Some(&b'[') {
                let Some(end) = find_style_end(format, hashes_end + 1) else {
                    break;
                };
                if self.tracing() {
                    self.log(depth + 1, &format!("found #*{}[", hashes_end - index));
                }
                self.flush(&mut output, &mut literal);
                output.extend_from_slice(&format.as_bytes()[index..=hashes_end]);
                let style = self.expand_style_body(&format[hashes_end + 1..end], depth);
                output.extend_from_slice(style.as_bytes());
                output.push(b']');
                index = end + 1;
                continue;
            }
            let next_index = index + 1;
            let Some(next) = format[next_index..].chars().next() else {
                break;
            };
            let after_next = next_index + next.len_utf8();
            match next {
                '#' => {
                    literal.push(next);
                    index = after_next;
                }
                '}' | ',' => {
                    if self.tracing() {
                        self.log(depth + 1, &format!("found #{next}"));
                    }
                    literal.push(next);
                    index = after_next;
                }
                '[' => unreachable!("style openers are handled before format tokens"),
                '(' => {
                    let Some(end) = find_plain_group_end(format, after_next, '(', ')') else {
                        break;
                    };
                    self.flush(&mut output, &mut literal);
                    if self.tracing() {
                        self.log(
                            depth + 1,
                            &format!("found #(): {}", &format[after_next..end]),
                        );
                    }
                    let out = self.hooks.shell(&format[after_next..end], &self.job_tag);
                    if self.tracing() {
                        self.log(depth + 1, &format!("#() result: {out}"));
                    }
                    output.extend_from_slice(out.as_bytes());
                    index = end + 1;
                }
                '{' => {
                    let Some(end) = find_format_end(format, index) else {
                        break;
                    };
                    self.flush(&mut output, &mut literal);
                    if self.tracing() {
                        self.log(
                            depth + 1,
                            &format!("found #{{}}: {}", &format[after_next..end]),
                        );
                    }
                    match self.expand_replacement(&format[after_next..end], depth + 1) {
                        Ok(value) => output.extend_from_slice(value.as_bytes()),
                        Err(()) => break,
                    }
                    index = end + 1;
                }
                _ => {
                    if let Some(name) = shorthand(next) {
                        self.flush(&mut output, &mut literal);
                        let found = self.context.variable(name);
                        if self.tracing() {
                            self.log(depth + 1, &format!("found #{next}: {name}"));
                            match found.as_deref() {
                                Some(value) => {
                                    self.log(depth + 1, &format!("format '{name}' found: {value}"));
                                }
                                None => self.log(depth + 1, &format!("format '{name}' not found")),
                            }
                            let value = found.clone().unwrap_or_default();
                            self.log(depth + 1, &format!("replaced '{name}' with '{value}'"));
                        }
                        output.extend_from_slice(found.unwrap_or_default().as_bytes());
                    } else {
                        literal.push('#');
                        literal.push(next);
                    }
                    index = after_next;
                }
            }
        }
        self.flush(&mut output, &mut literal);
        let output = RawText::from_bytes(output);
        if self.tracing() {
            self.log(depth + 1, &format!("result is: {output}"));
        }
        output
    }

    fn expand_style_body(&mut self, body: &str, depth: usize) -> String {
        let mut output = String::with_capacity(body.len());
        let mut index = 0;
        while index < body.len() {
            if body.as_bytes()[index] != b'#' {
                let character = body[index..]
                    .chars()
                    .next()
                    .expect("index is on a character boundary");
                output.push(character);
                index += character.len_utf8();
                continue;
            }
            let Some(next) = body.as_bytes().get(index + 1).copied() else {
                break;
            };
            match next {
                b'{' => {
                    let Some(end) = find_format_end(body, index) else {
                        break;
                    };
                    if self.tracing() {
                        self.log(
                            depth + 1,
                            &format!("found #{{}}: {}", &body[index + 2..end]),
                        );
                    }
                    match self.expand_replacement(&body[index + 2..end], depth + 1) {
                        Ok(value) => output.push_str(&value),
                        Err(()) => break,
                    }
                    index = end + 1;
                }
                b'(' => {
                    let Some(end) = find_plain_group_end(body, index + 2, '(', ')') else {
                        break;
                    };
                    let out = self.hooks.shell(&body[index + 2..end], &self.job_tag);
                    output.push_str(&out);
                    index = end + 1;
                }
                b'#' | b'}' | b',' => {
                    output.push(char::from(next));
                    index += 2;
                }
                _ => {
                    output.push('#');
                    output.push(char::from(next));
                    index += 2;
                }
            }
        }
        output
    }

    fn flush(&mut self, output: &mut Vec<u8>, literal: &mut String) {
        if literal.is_empty() {
            return;
        }
        output.extend_from_slice(literal.as_bytes());
        literal.clear();
    }

    fn reexpand(&mut self, value: RawText, depth: usize) -> RawText {
        let expanded = self.expand(&value, depth);
        if expanded.as_str() == value.as_str() {
            value
        } else {
            expanded
        }
    }

    fn expand_replacement(&mut self, body: &str, depth: usize) -> Result<RawText, ()> {
        let value = self.expand_replacement_inner(body, depth);
        if self.tracing() {
            match value.as_deref() {
                Ok(value) => self.log(depth, &format!("replaced '{body}' with '{value}'")),
                Err(()) => self.log(depth, &format!("failed {body}")),
            }
        }
        value
    }

    fn expand_replacement_inner(&mut self, body: &str, depth: usize) -> Result<RawText, ()> {
        let (modifiers, copy) = self.build_modifiers(body, depth).map_or_else(
            || (Vec::new(), body),
            |(modifiers, offset)| (modifiers, &body[offset..]),
        );
        if self.tracing() {
            for (index, modifier) in modifiers.iter().enumerate() {
                let text = modifier.text;
                let arguments = modifier.args.clone();
                self.log(depth, &format!("modifier {index} is {text}"));
                for (argument, value) in arguments.iter().enumerate() {
                    self.log(
                        depth,
                        &format!("modifier {index} argument {argument}: {value}"),
                    );
                }
            }
        }
        let flags = ModifierFlags::from_modifiers(&modifiers);
        self.expand_replacement_value(copy, depth, &flags)
    }

    fn expand_replacement_value(
        &mut self,
        copy: &str,
        depth: usize,
        flags: &ModifierFlags<'_>,
    ) -> Result<RawText, ()> {
        let mut value = if let Some(word) = flags.interrogate {
            self.expand_interrogate(copy, word)
        } else if let Some(count) = flags.cycle {
            let value = self.expand_cycle(copy, count);
            if self.tracing() {
                self.log(depth, &format!("cycle '{copy}' is: {value}"));
            }
            RawText::from(value)
        } else if flags.literal {
            if self.tracing() {
                self.log(depth, &format!("literal string is '{copy}'"));
            }
            RawText::from(unescape(copy))
        } else if flags.character {
            RawText::from(self.expand_character(copy, depth))
        } else if let Some(colour_flags) = flags.colour {
            RawText::from(self.expand_colour(copy, depth, colour_flags))
        } else if flags.loop_sessions {
            RawText::from(self.expand_loop(copy, depth, LoopTarget::Sessions, flags)?)
        } else if flags.loop_windows {
            RawText::from(self.expand_loop(copy, depth, LoopTarget::Windows, flags)?)
        } else if flags.loop_panes {
            RawText::from(self.expand_loop(copy, depth, LoopTarget::Panes, flags)?)
        } else if flags.loop_clients {
            RawText::from(self.expand_client_loop(copy, depth, flags))
        } else if let Some(loop_flags) = flags.loop_options {
            RawText::from(self.expand_option_loop(copy, depth, loop_flags))
        } else if let Some(loop_flags) = flags.loop_environment {
            RawText::from(self.expand_environment_loop(copy, depth, loop_flags))
        } else if flags.name_window {
            RawText::from(self.expand_name_exists(copy, depth, true)?)
        } else if flags.name_session {
            RawText::from(self.expand_name_exists(copy, depth, false)?)
        } else if let Some(search_flags) = flags.content_search {
            RawText::from(self.expand_content_search(copy, depth, search_flags))
        } else if flags.repeat {
            RawText::from(self.expand_repeat(copy, depth)?)
        } else if flags.not {
            RawText::from(bool_string(!format_true(&self.expand(copy, depth))))
        } else if flags.not_not {
            RawText::from(bool_string(format_true(&self.expand(copy, depth))))
        } else if let Some(and) = flags.bool_op {
            RawText::from(self.expand_boolean(copy, depth, and))
        } else if let Some(comparison) = flags.comparison {
            RawText::from(self.expand_comparison(copy, depth, comparison)?)
        } else if let Some(conditional) = copy.strip_prefix('?') {
            self.expand_conditional(conditional, depth, flags)
        } else if let Some(expression) = flags.expression {
            RawText::from(self.expand_expression(copy, depth, expression))
        } else if copy.contains("#{") {
            if self.tracing() {
                self.log(depth, &format!("expanding inner format '{copy}'"));
            }
            self.expand(copy, depth)
        } else {
            let found = self.lookup(copy, flags);
            if self.tracing() {
                match found.as_deref() {
                    Some(value) => self.log(depth, &format!("format '{copy}' found: {value}")),
                    None => self.log(depth, &format!("format '{copy}' not found")),
                }
            }
            found.unwrap_or_default()
        };

        if flags.expand {
            value = self.reexpand(value, depth);
        } else if flags.expand_time {
            let previous = std::mem::replace(&mut self.time, true);
            value = self.reexpand(value, depth);
            self.time = previous;
        }
        for substitution in &flags.substitutions {
            let pattern = self.expand(&substitution.args[0], depth);
            let replacement = self.expand(&substitution.args[1], depth);
            value = RawText::from(substitute(
                &value,
                &pattern,
                &replacement,
                substitution
                    .args
                    .get(2)
                    .is_some_and(|flags| flags.contains('i')),
            ));
            if self.tracing() {
                self.log(
                    depth,
                    &format!("substitute '{pattern}' to '{replacement}': {value}"),
                );
            }
        }
        if let Some((limit, marker)) = &flags.limit {
            value = truncate_value(&value, *limit, *marker);
            if self.tracing() {
                self.log(depth, &format!("applied length limit {limit}: {value}"));
            }
        }
        if flags.padding != 0 {
            value = pad_value(&value, flags.padding);
            if self.tracing() {
                let width = flags.padding;
                self.log(depth, &format!("applied padding width {width}: {value}"));
            }
        }
        if flags.length {
            value = RawText::from(value.byte_len().to_string());
            if self.tracing() {
                self.log(depth, &format!("replacing with length: {value}"));
            }
        }
        if flags.width {
            value = RawText::from(styled_display_width(value.as_bytes()).to_string());
            if self.tracing() {
                self.log(depth, &format!("replacing with width: {value}"));
            }
        }
        Ok(value)
    }

    fn build_modifiers(&mut self, body: &str, depth: usize) -> Option<(Vec<Modifier>, usize)> {
        parse_modifiers(body, |value| self.expand(value, depth).to_string())
    }

    fn split_once<'s>(&self, source: &'s str) -> (&'s str, Option<&'s str>) {
        if self.tracing() || !compiled::enabled() {
            return split_once_top(source, ',');
        }
        let template = compiled::get(source);
        let first = &template.parts[0];
        (
            &source[first.clone()],
            (template.parts.len() > 1).then(|| &source[first.end + 1..]),
        )
    }

    fn expand_character(&mut self, copy: &str, depth: usize) -> String {
        self.expand(copy, depth)
            .parse::<u8>()
            .ok()
            .filter(|value| (32..=126).contains(value))
            .map(char::from)
            .map_or_else(String::new, |value| value.to_string())
    }

    fn expand_interrogate(&mut self, copy: &str, word: &str) -> RawText {
        let Some(terminal) = self.hooks.client_tty_term() else {
            return RawText::default();
        };
        let mut value = RawText::default();
        if word.contains('c') {
            value = RawText::from(bool_string(terminal.has_capability(copy)));
        }
        if word.contains('f') {
            value = RawText::from(bool_string(terminal.has_feature(copy)));
        }
        if word.contains('e') {
            value = self
                .hooks
                .client_terminal_environment()
                .into_iter()
                .find(|row| row.name == copy)
                .map(|row| row.value)
                .unwrap_or_default();
        }
        value
    }

    fn expand_colour(&mut self, copy: &str, depth: usize, flags: &str) -> String {
        let value = self.expand(copy, depth);
        format_colour(&value, flags)
    }

    fn expand_name_exists(
        &mut self,
        copy: &str,
        depth: usize,
        windows: bool,
    ) -> Result<String, ()> {
        let name = self.expand(copy, depth);
        let exists = if windows {
            let session = &self.context.values().session_id;
            if session.is_empty() {
                return Err(());
            }
            self.universe
                .windows(session)
                .is_some_and(|items| items.iter().any(|item| item.metadata.window_name == name))
        } else {
            self.universe
                .sessions()
                .iter()
                .any(|item| item.metadata.session_name == name)
        };
        Ok(bool_string(exists).to_owned())
    }

    fn expand_content_search(&mut self, copy: &str, depth: usize, flags: &str) -> String {
        let pattern = self.expand(copy, depth);
        let pane = self
            .context
            .values()
            .pane_id
            .strip_prefix('%')
            .and_then(|value| value.parse::<u64>().ok())
            .map(PaneId);
        self.hooks
            .pane_search(pane, &pattern, flags.contains('r'), flags.contains('i'))
            .to_string()
    }

    fn expand_repeat(&mut self, copy: &str, depth: usize) -> Result<String, ()> {
        let (value, count) = self.split_once(copy);
        let count = count.ok_or(())?;
        let value = self.expand(value, depth);
        let count = self
            .expand(count, depth)
            .trim_start()
            .parse::<usize>()
            .ok()
            .filter(|count| (1..=FORMAT_MAX_REPEAT).contains(count));
        let Some(count) = count else {
            return Ok(String::new());
        };
        let Some(bytes) = value
            .len()
            .checked_mul(count)
            .filter(|bytes| *bytes <= FORMAT_MAX_REPEAT_BYTES)
        else {
            return Err(());
        };
        let mut repeated = String::new();
        repeated.try_reserve_exact(bytes).map_err(|_| ())?;
        for _ in 0..count {
            repeated.push_str(&value);
        }
        Ok(repeated)
    }

    fn expand_expression(&mut self, copy: &str, depth: usize, modifier: &Modifier) -> String {
        let Some(operator) = modifier.args.first() else {
            return String::new();
        };
        let floating = modifier
            .args
            .get(1)
            .is_some_and(|flags| flags.contains('f'));
        let precision = if let Some(value) = modifier.args.get(2) {
            let Some(value) = value
                .parse::<i32>()
                .ok()
                .filter(|value| (-100..=100).contains(value))
            else {
                return String::new();
            };
            value
        } else if floating {
            2
        } else {
            0
        };
        let (left, right) = self.split_once(copy);
        let Some(right) = right else {
            return String::new();
        };
        let Some(mut left) = parse_expression_number(&self.expand(left, depth)) else {
            return String::new();
        };
        let Some(mut right) = parse_expression_number(&self.expand(right, depth)) else {
            return String::new();
        };
        if !floating {
            left = left as i64 as f64;
            right = right as i64 as f64;
        }
        let result = match operator.as_str() {
            "+" => left + right,
            "-" => left - right,
            "*" => left * right,
            "/" => left / right,
            "m" | "%" => left % right,
            "==" => {
                if (left - right).abs() < 1e-9 {
                    1.0
                } else {
                    0.0
                }
            }
            "!=" => {
                if (left - right).abs() > 1e-9 {
                    1.0
                } else {
                    0.0
                }
            }
            ">" => {
                if left > right {
                    1.0
                } else {
                    0.0
                }
            }
            "<" => {
                if left < right {
                    1.0
                } else {
                    0.0
                }
            }
            ">=" => {
                if left >= right {
                    1.0
                } else {
                    0.0
                }
            }
            "<=" => {
                if left <= right {
                    1.0
                } else {
                    0.0
                }
            }
            _ => return String::new(),
        };
        let result = if floating {
            result
        } else {
            result as i64 as f64
        };
        let precision = usize::try_from(precision).unwrap_or(6);
        format!("{result:.precision$}")
    }

    fn expand_loop(
        &mut self,
        copy: &str,
        depth: usize,
        target: LoopTarget,
        flags: &ModifierFlags<'_>,
    ) -> Result<String, ()> {
        let items = match target {
            LoopTarget::Sessions => self.universe.sessions(),
            LoopTarget::Windows => self
                .universe
                .windows(&self.context.values().session_id)
                .ok_or(())?,
            LoopTarget::Panes => self
                .universe
                .panes(&self.context.values().window_id)
                .ok_or(())?,
        };
        let mut items = items.iter().collect::<Vec<_>>();
        sort_loop_items(&mut items, target, flags.loop_sort);
        if flags.loop_reversed {
            items.reverse();
        }
        let (all, active) = self.split_once(copy);
        let mut output = String::new();
        let last = items.len().saturating_sub(1);
        for index in 0..items.len() {
            let item = items[index];
            let mut dynamic = BTreeMap::from([
                (LOOP_CONTEXT_FORMATS[0].to_owned(), index.to_string()),
                (
                    LOOP_CONTEXT_FORMATS[1].to_owned(),
                    bool_string(index == last).to_owned(),
                ),
            ]);
            if target == LoopTarget::Windows {
                dynamic.insert(
                    WINDOW_LOOP_CONTEXT_FORMATS[0].to_owned(),
                    bool_string(index > 0 && items[index - 1].active).to_owned(),
                );
                dynamic.insert(
                    WINDOW_LOOP_CONTEXT_FORMATS[1].to_owned(),
                    bool_string(index + 1 < items.len() && items[index + 1].active).to_owned(),
                );
                if let Some(next) = items.get(index + 1) {
                    dynamic.insert(
                        WINDOW_NEIGHBOUR_INDEX_CONTEXT_FORMATS[0].to_owned(),
                        next.metadata.window_index.to_string(),
                    );
                    dynamic.insert(
                        WINDOW_NEIGHBOUR_ACTIVE_CONTEXT_FORMATS[0].to_owned(),
                        bool_string(next.active).to_owned(),
                    );
                    insert_window_neighbour_user_options(
                        &mut dynamic,
                        self.universe,
                        next,
                        WINDOW_NEIGHBOUR_USER_OPTION_PREFIXES[0],
                    );
                }
                if index > 0 {
                    let previous = items[index - 1];
                    dynamic.insert(
                        WINDOW_NEIGHBOUR_INDEX_CONTEXT_FORMATS[1].to_owned(),
                        previous.metadata.window_index.to_string(),
                    );
                    dynamic.insert(
                        WINDOW_NEIGHBOUR_ACTIVE_CONTEXT_FORMATS[1].to_owned(),
                        bool_string(previous.active).to_owned(),
                    );
                    insert_window_neighbour_user_options(
                        &mut dynamic,
                        self.universe,
                        previous,
                        WINDOW_NEIGHBOUR_USER_OPTION_PREFIXES[1],
                    );
                }
            }
            let mut child = item.context.borrowed_child(self.universe.engine);
            child.format_now = self.context.values().format_now;
            if let Some(values) = child.values.get_mut() {
                values.format_now = child.format_now;
            }
            let variables = LoopVariables {
                context: &child,
                format_type: target.format_type(),
                dynamic,
            };
            let selected = if item.active {
                active.unwrap_or(all)
            } else {
                all
            };
            let mut expander = Expander {
                context: &variables,
                universe: self.universe,
                hooks: &mut *self.hooks,
                time: self.time,
                job_tag: loop_job_tag(&self.job_tag, target, variables.context.values()),
                client_row: self.client_row,
                trace: self.trace.as_deref_mut(),
            };
            output.push_str(&expander.expand(selected, depth));
        }
        Ok(output)
    }

    fn expand_client_loop(
        &mut self,
        copy: &str,
        depth: usize,
        flags: &ModifierFlags<'_>,
    ) -> String {
        let mut rows = self.hooks.client_loop_rows(self.context.values());
        sort_client_loop_rows(&mut rows, flags.loop_sort, flags.loop_reversed);
        let outer = self.context;
        let context = outer.values();
        let format_type = outer.format_type();
        let mut output = String::new();
        let last = rows.len().saturating_sub(1);
        for (index, row) in rows.iter().enumerate() {
            let variables = LoopVariables {
                context,
                format_type,
                dynamic: BTreeMap::from([
                    (LOOP_CONTEXT_FORMATS[0].to_owned(), index.to_string()),
                    (
                        LOOP_CONTEXT_FORMATS[1].to_owned(),
                        bool_string(index == last).to_owned(),
                    ),
                ]),
            };
            let mut expander = Expander {
                context: &variables,
                universe: self.universe,
                hooks: &mut *self.hooks,
                time: self.time,
                job_tag: self.job_tag.clone(),
                client_row: Some(row),
                trace: self.trace.as_deref_mut(),
            };
            output.push_str(&expander.expand(copy, depth));
        }
        output
    }

    fn expand_environment_loop(&mut self, copy: &str, depth: usize, loop_flags: &str) -> String {
        let outer = self.context;
        let context = outer.values();
        let rows: Arc<[FormatEnvironRow]> = match loop_flags {
            "" | "s" => self
                .universe
                .session_environment(&context.session_id)
                .unwrap_or_default(),
            "g" => self.universe.global_environment(),
            "c" => self
                .client_row
                .map_or_else(
                    || self.hooks.client_environment_rows(),
                    |row| row.environment.clone(),
                )
                .into(),
            _ => return String::new(),
        };
        let format_type = outer.format_type();
        let mut output = String::new();
        let last = rows.len().saturating_sub(1);
        for (index, row) in rows.iter().enumerate() {
            let variables = LoopVariables {
                context,
                format_type,
                dynamic: environment_loop_variables(row, index, index == last),
            };
            let mut expander = Expander {
                context: &variables,
                universe: self.universe,
                hooks: &mut *self.hooks,
                time: self.time,
                job_tag: self.job_tag.clone(),
                client_row: self.client_row,
                trace: self.trace.as_deref_mut(),
            };
            output.push_str(&expander.expand(copy, depth));
        }
        output
    }

    fn expand_option_loop(&mut self, copy: &str, depth: usize, loop_flags: &str) -> String {
        let outer = self.context;
        let context = outer.values();
        let Some(rows) =
            option_loop_key(loop_flags, context).and_then(|key| self.universe.option_rows(key))
        else {
            return String::new();
        };
        let mut output = String::new();
        let last = rows.len().saturating_sub(1);
        for (index, row) in rows.iter().enumerate() {
            let variables = LoopVariables {
                context,
                format_type: FormatType::None,
                dynamic: option_loop_variables(row, index, index == last),
            };
            let mut expander = Expander {
                context: &variables,
                universe: self.universe,
                hooks: &mut *self.hooks,
                time: self.time,
                job_tag: self.job_tag.clone(),
                client_row: self.client_row,
                trace: self.trace.as_deref_mut(),
            };
            output.push_str(&expander.expand(copy, depth));
        }
        output
    }

    fn expand_boolean(&mut self, copy: &str, depth: usize, and: bool) -> String {
        let mut result = and;
        let mut rest = copy;
        loop {
            let (operand, next) = self.split_once(rest);
            let truth = format_true(&self.expand(operand, depth));
            result = if and {
                result && truth
            } else {
                result || truth
            };
            if result != and || next.is_none() {
                break;
            }
            rest = next.unwrap_or_default();
        }
        bool_string(result).to_owned()
    }

    fn expand_comparison(
        &mut self,
        copy: &str,
        depth: usize,
        comparison: Comparison,
    ) -> Result<String, ()> {
        let (left, right) = self.split_once(copy);
        let Some(right) = right else {
            if self.tracing() {
                let text = comparison_text(comparison);
                self.log(depth, &format!("compare {text} syntax error: {copy}"));
            }
            return Err(());
        };
        let left = self.expand(left, depth);
        let right = self.expand(right, depth);
        if self.tracing() {
            let text = comparison_text(comparison);
            self.log(depth, &format!("compare {text} left is: {left}"));
            self.log(depth, &format!("compare {text} right is: {right}"));
        }
        let value = match comparison {
            Comparison::Equal => bool_string(left == right).to_owned(),
            Comparison::NotEqual => bool_string(left != right).to_owned(),
            Comparison::Less => bool_string(left.cmp(&right) == Ordering::Less).to_owned(),
            Comparison::Greater => bool_string(left.cmp(&right) == Ordering::Greater).to_owned(),
            Comparison::LessEqual => bool_string(left.cmp(&right) != Ordering::Greater).to_owned(),
            Comparison::GreaterEqual => bool_string(left.cmp(&right) != Ordering::Less).to_owned(),
            Comparison::Match(flags) => match_value(&left, &right, flags),
        };
        Ok(value)
    }

    fn expand_conditional(
        &mut self,
        copy: &str,
        depth: usize,
        flags: &ModifierFlags<'_>,
    ) -> RawText {
        let compiled = (compiled::enabled() && !self.tracing()).then(|| compiled::get(copy));
        let parts = compiled.is_none().then(|| split_top(copy, ','));
        let count = compiled.as_ref().map_or_else(
            || parts.as_ref().unwrap().len(),
            |template| template.parts.len(),
        );
        let part = |index: usize| {
            compiled.as_ref().map_or_else(
                || parts.as_ref().unwrap()[index],
                |template| &copy[template.parts[index].clone()],
            )
        };
        let paired = count / 2 * 2;
        for index in (0..paired).step_by(2) {
            let condition = part(index);
            if self.tracing() {
                self.log(depth, &format!("condition is: {condition}"));
            }
            let found = if let Some(value) = self.lookup(condition, flags) {
                if self.tracing() {
                    self.log(depth, &format!("condition '{condition}' found: {value}"));
                }
                value
            } else {
                let expanded = self.expand(condition, depth);
                if expanded == condition {
                    if self.tracing() {
                        self.log(
                            depth,
                            &format!("condition '{condition}' not found; assuming false"),
                        );
                    }
                    RawText::default()
                } else {
                    expanded
                }
            };
            let truthy = format_true(&found);
            if self.tracing() {
                let state = if truthy { "true" } else { "false" };
                self.log(depth, &format!("condition '{condition}' is {state}"));
            }
            if truthy {
                return self.expand(part(index + 1), depth);
            }
        }
        if count % 2 == 1 {
            if self.tracing() {
                self.log(
                    depth,
                    &format!("no condition matched in '{copy}'; using last arg"),
                );
            }
            self.expand(part(count - 1), depth)
        } else {
            if self.tracing() {
                self.log(
                    depth,
                    &format!("no condition matched in '{copy}'; using empty string"),
                );
            }
            RawText::default()
        }
    }

    fn environment_value(&mut self, key: &str) -> Option<RawText> {
        let session = self
            .universe
            .session_environment(&self.context.values().session_id);
        if let Some(row) = session
            .as_deref()
            .and_then(|rows| rows.iter().find(|row| row.name == key))
        {
            return (!row.removed).then(|| row.value.clone());
        }
        let global = self.universe.global_environment();
        let row = global.iter().find(|row| row.name == key)?;
        (!row.removed).then(|| row.value.clone())
    }

    fn append_lookup(&mut self, key: &str, output: &mut Vec<u8>, padding: isize) {
        if let Some(value) = self.hooks.option_variable(key, self.context.values()) {
            append_padded(output, value.as_bytes(), padding);
            return;
        }
        if let Some(row) = self.client_row.filter(|_| client_loop_scoped(key)) {
            append_padded(
                output,
                row.variables.get(key).map_or(b"", |value| value.as_bytes()),
                padding,
            );
            return;
        }
        if let Some(value) = self.hooks.variable(key, self.context.values()) {
            append_padded(output, value.as_bytes(), padding);
            return;
        }
        if self.context.variable_kind(key).is_some() {
            if let Some(value) = self.context.variable(key) {
                append_padded(output, value.as_bytes(), padding);
            } else {
                append_padded(output, b"", padding);
            }
            return;
        }
        if let Some(value) = self.hooks.tree_variable(key, self.context.values()) {
            append_padded(output, value.as_bytes(), padding);
            return;
        }
        append_padded(
            output,
            self.environment_value(key).unwrap_or_default().as_bytes(),
            padding,
        );
    }

    pub(super) fn lookup(&mut self, key: &str, flags: &ModifierFlags<'_>) -> Option<RawText> {
        let hooked = self
            .hooks
            .option_variable(key, self.context.values())
            .or_else(|| match self.client_row {
                Some(row) if client_loop_scoped(key) => Some(RawText::from(
                    row.variables.get(key).cloned().unwrap_or_default(),
                )),
                _ => self
                    .hooks
                    .variable(key, self.context.values())
                    .map(RawText::from),
            });
        let mut value = if let Some(hooked) = hooked {
            hooked
        } else if self.context.variable_kind(key).is_some() {
            RawText::from(self.context.variable(key).map(Cow::into_owned)?)
        } else if let Some(value) = self.hooks.tree_variable(key, self.context.values()) {
            RawText::from(value.into_owned())
        } else if flags.time.enabled {
            return None;
        } else {
            self.environment_value(key)?
        };
        if flags.time.enabled {
            return Some(RawText::from(format_time_value(
                &value,
                &flags.time,
                self.context.values().format_now,
            )));
        }
        if flags.basename {
            value = basename(value.as_bytes());
        }
        if flags.dirname {
            value = dirname(value.as_bytes());
        }
        if flags.quote_shell {
            value = quote_shell(value.as_bytes());
        }
        if flags.quote_single {
            value = quote_single(value.as_bytes());
        }
        if flags.quote_style {
            value = quote_style(value.as_bytes());
        }
        if flags.quote_arguments {
            value = RawText::from(quote_argument(&value));
        }
        Some(value)
    }
}
