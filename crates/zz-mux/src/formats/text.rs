use super::*;

pub(super) fn session_number(value: &str) -> u64 {
    value
        .strip_prefix('$')
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default()
}

pub(super) fn parse_expression_number(value: &str) -> Option<f64> {
    if value.is_empty() {
        return Some(0.0);
    }
    let value = value.trim_start();
    if value.trim_end() != value {
        return None;
    }
    value.parse::<f64>().ok()
}

pub(super) fn pad_value(value: &RawText, width: isize) -> RawText {
    let target = width.unsigned_abs();
    let current = format_byte_width(value.as_bytes());
    if current >= target {
        return value.clone();
    }
    let padding = " ".repeat(target - current);
    let mut output = Vec::with_capacity(value.byte_len() + padding.len());
    if width < 0 {
        output.extend_from_slice(padding.as_bytes());
        output.extend_from_slice(value.as_bytes());
    } else {
        output.extend_from_slice(value.as_bytes());
        output.extend_from_slice(padding.as_bytes());
    }
    RawText::from_bytes(output)
}

pub(super) fn format_colour(value: &str, flags: &str) -> String {
    if flags.contains('f') || flags.contains('b') {
        if value.eq_ignore_ascii_case("none") {
            return "\u{1b}[0m".to_owned();
        }
        return parse_tmux_colour(value)
            .and_then(|colour| colour_escape(colour, flags.contains('b')))
            .unwrap_or_default();
    }
    parse_tmux_colour(value)
        .and_then(colour_rgb)
        .map_or_else(String::new, |colour| format!("{colour:06x}"))
}

fn colour_rgb(colour: TmuxColour) -> Option<u32> {
    match colour {
        TmuxColour::Basic(index) | TmuxColour::Indexed(index) => Some(indexed_colour_rgb(index)),
        TmuxColour::Rgb(colour) => Some(colour),
        TmuxColour::Default | TmuxColour::Terminal | TmuxColour::Theme(_) => None,
    }
}

fn colour_escape(colour: TmuxColour, background: bool) -> Option<String> {
    let colour = match colour {
        TmuxColour::Theme(index) => {
            let basic = [0, 7, 7, 0, 2, 3, 1, 4, 6, 5].get(usize::from(index))?;
            TmuxColour::Basic(*basic)
        }
        colour => colour,
    };
    let base = if background { 40 } else { 30 };
    Some(match colour {
        TmuxColour::Basic(index) if index < 8 => format!("\u{1b}[{}m", base + index),
        TmuxColour::Basic(index) => format!("\u{1b}[{}m", base + 60 + index - 8),
        TmuxColour::Indexed(index) => format!("\u{1b}[{};5;{index}m", base + 8),
        TmuxColour::Rgb(colour) => format!(
            "\u{1b}[{};2;{};{};{}m",
            base + 8,
            (colour >> 16) & 0xff,
            (colour >> 8) & 0xff,
            colour & 0xff
        ),
        TmuxColour::Default | TmuxColour::Terminal => format!("\u{1b}[{}m", base + 9),
        TmuxColour::Theme(_) => return None,
    })
}

pub(super) fn shorthand(character: char) -> Option<&'static str> {
    Some(match character {
        'D' => "pane_id",
        'F' => "window_flags",
        'H' => "host",
        'I' => "window_index",
        'P' => "pane_index",
        'S' => "session_name",
        'T' => "pane_title",
        'W' => "window_name",
        'h' => "host_short",
        _ => return None,
    })
}

pub(super) const fn is_modifier_end(character: u8) -> bool {
    character == b';' || character == b':'
}

pub(super) fn find_plain_group_end(
    text: &str,
    start: usize,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 1usize;
    for (offset, character) in text[start..].char_indices() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth -= 1;
            if depth == 0 {
                return Some(start + offset);
            }
        }
    }
    None
}

pub(super) fn find_style_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut position = start;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            position = find_format_end(text, position)? + 1;
            continue;
        }
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'(') {
            position = find_plain_group_end(text, position + 2, '(', ')')? + 1;
            continue;
        }
        if bytes[position] == b']' {
            return Some(position);
        }
        position += 1;
    }
    None
}

pub(super) fn find_format_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut position = start;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            depth += 1;
            position += 2;
            continue;
        }
        if bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            position += 2;
            continue;
        }
        if bytes[position] == b'}' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(position);
            }
        }
        position += 1;
    }
    None
}

pub(super) fn styled_display_width(bytes: &[u8]) -> usize {
    let mut width = 0usize;
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'#' {
            let (end, leading) = leading_hashes(bytes, position);
            width += leading;
            position = end;
            if bytes.get(position) == Some(&b'#') {
                let Some(end) = find_measured_style_end(bytes, position + 2) else {
                    return 0;
                };
                position = end + 1;
            }
            continue;
        }
        let (length, unit, _) = next_format_unit(bytes, position);
        width += unit;
        position += length;
    }
    width
}

fn leading_hashes(bytes: &[u8], start: usize) -> (usize, usize) {
    let mut count = 0usize;
    while bytes.get(start + count) == Some(&b'#') {
        count += 1;
    }
    if count == 0 {
        return (start, 0);
    }
    if bytes.get(start + count) != Some(&b'[') {
        return (start + count, count.div_ceil(2));
    }
    if count.is_multiple_of(2) {
        (start + count, count / 2)
    } else {
        (start + count - 1, count / 2)
    }
}

fn find_measured_style_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut brackets = 0isize;
    let mut position = start;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            brackets += 1;
        }
        if bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            position += 2;
            continue;
        }
        if bytes[position] == b'}' {
            brackets -= 1;
        }
        if bytes[position] == b']' && brackets == 0 {
            return Some(position);
        }
        position += 1;
    }
    None
}

pub(super) fn find_modifier_argument(text: &str, start: usize, wrapper: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0isize;
    let mut position = start;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            depth += 1;
            position += 2;
            continue;
        }
        if bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            position += 2;
            continue;
        }
        if bytes[position] == b'}' {
            depth -= 1;
            position += 1;
            continue;
        }
        if depth == 0
            && (is_modifier_end(bytes[position]) || (wrapper != 0 && bytes[position] == wrapper))
        {
            return Some(position);
        }
        position += 1;
    }
    None
}

pub(super) fn split_once_top(text: &str, separator: char) -> (&str, Option<&str>) {
    let bytes = text.as_bytes();
    let separator = separator as u8;
    let mut depth = 0usize;
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            depth += 1;
            position += 2;
            continue;
        }
        if bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            position += 2;
            continue;
        }
        if bytes[position] == b'}' && depth > 0 {
            depth -= 1;
        } else if bytes[position] == separator && depth == 0 {
            return (&text[..position], Some(&text[position + 1..]));
        }
        position += 1;
    }
    (text, None)
}

pub(super) fn split_top(text: &str, separator: char) -> Vec<&str> {
    let mut values = Vec::new();
    let mut rest = text;
    loop {
        let (value, next) = split_once_top(rest, separator);
        values.push(value);
        let Some(next) = next else {
            break;
        };
        rest = next;
    }
    values
}

pub(super) fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut depth = 0usize;
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            depth += 1;
        }
        if depth == 0
            && bytes[position] == b'#'
            && bytes
                .get(position + 1)
                .is_some_and(|next| matches!(*next, b',' | b'#' | b'{' | b'}' | b':'))
        {
            output.push(bytes[position + 1] as char);
            position += 2;
            continue;
        }
        if bytes[position] == b'}' {
            depth = depth.saturating_sub(1);
        }
        let character = text[position..].chars().next().unwrap_or_default();
        output.push(character);
        position += character.len_utf8();
    }
    output
}

pub fn format_true(value: &str) -> bool {
    !value.is_empty() && value != "0"
}

fn trim_end_slashes(value: &[u8]) -> &[u8] {
    let end = value
        .iter()
        .rposition(|byte| *byte != b'/')
        .map_or(0, |index| index + 1);
    &value[..end]
}

pub(super) fn basename(value: &[u8]) -> RawText {
    if value.is_empty() {
        return RawText::from(".");
    }
    let trimmed = trim_end_slashes(value);
    if trimmed.is_empty() {
        return RawText::from("/");
    }
    let start = trimmed
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |index| index + 1);
    RawText::from_bytes(&trimmed[start..])
}

pub(super) fn dirname(value: &[u8]) -> RawText {
    if value.is_empty() {
        return RawText::from(".");
    }
    let trimmed = trim_end_slashes(value);
    if trimmed.is_empty() {
        return RawText::from("/");
    }
    let Some(index) = trimmed.iter().rposition(|byte| *byte == b'/') else {
        return RawText::from(".");
    };
    let parent = trim_end_slashes(&trimmed[..index]);
    if parent.is_empty() {
        RawText::from("/")
    } else {
        RawText::from_bytes(parent)
    }
}

pub(super) fn quote_shell(value: &[u8]) -> RawText {
    let mut output = Vec::with_capacity(value.len() * 2);
    for byte in value {
        if b"|&;<>(){}$`\\\"'*?[# =%\n\t".contains(byte) {
            output.push(b'\\');
        }
        output.push(*byte);
    }
    RawText::from_bytes(output)
}

pub(super) fn quote_single(value: &[u8]) -> RawText {
    let mut output = Vec::with_capacity(value.len() + 2);
    output.push(b'\'');
    for byte in value {
        if *byte == b'\'' {
            output.extend_from_slice(b"'\\''");
        } else {
            output.push(*byte);
        }
    }
    output.push(b'\'');
    RawText::from_bytes(output)
}

pub(super) fn quote_style(value: &[u8]) -> RawText {
    let mut output = Vec::with_capacity(value.len());
    for byte in value {
        if *byte == b'#' {
            output.push(b'#');
        }
        output.push(*byte);
    }
    RawText::from_bytes(output)
}

pub(crate) fn quote_argument(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    let double = value
        .chars()
        .any(|character| " #';${}%".contains(character));
    let single = !double && value.chars().any(|character| " \"".contains(character));
    if value != " " && value.len() == 1 && (double || single || value == "~") {
        return format!("\\{value}");
    }
    let mut escaped = String::with_capacity(value.len() * 2);
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\u{8}' => escaped.push_str("\\b"),
            '\u{7}' => escaped.push_str("\\a"),
            '\u{b}' => escaped.push_str("\\v"),
            '\t' => escaped.push_str("\\t"),
            '\u{c}' => escaped.push_str("\\f"),
            '\0' => {
                escaped.push_str("\\0");
                if characters
                    .peek()
                    .is_some_and(|next| matches!(next, '0'..='7'))
                {
                    escaped.push_str("00");
                }
            }
            '\\' => escaped.push_str("\\\\"),
            '"' if double => escaped.push_str("\\\""),
            '$' if double
                && characters.peek().is_some_and(|next| {
                    next.is_ascii_alphabetic() || matches!(next, '_' | '{')
                }) =>
            {
                escaped.push_str("\\$");
            }
            character if character.is_ascii_control() => {
                let _ = write!(&mut escaped, "\\{:03o}", u32::from(character));
            }
            character => escaped.push(character),
        }
    }
    if double {
        if escaped.starts_with('~') {
            format!("\"\\{escaped}\"")
        } else {
            format!("\"{escaped}\"")
        }
    } else if single {
        format!("'{escaped}'")
    } else if escaped.starts_with('~') {
        format!("\\{escaped}")
    } else {
        escaped
    }
}

fn next_format_unit(value: &[u8], index: usize) -> (usize, usize, bool) {
    let byte = value[index];
    let size = match byte {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    };
    if size != 0
        && let Some(slice) = value.get(index..index + size)
        && let Ok(text) = std::str::from_utf8(slice)
        && let Some(character) = text.chars().next()
        && character.len_utf8() == size
    {
        return (size, character.width().unwrap_or_default(), true);
    }
    if (0x20..0x7f).contains(&byte) {
        (1, 1, true)
    } else {
        (1, 0, false)
    }
}

#[must_use]
pub fn utf8_sanitize(value: &[u8]) -> String {
    let mut output = String::with_capacity(value.len());
    let mut index = 0usize;
    while index < value.len() {
        let (length, width, printable) = next_format_unit(value, index);
        if length > 1 {
            for _ in 0..width {
                output.push('_');
            }
        } else if printable {
            output.push(char::from(value[index]));
        } else {
            output.push('_');
        }
        index += length;
    }
    output
}

#[must_use]
pub fn sanitize_client_output(output: &RawText) -> RawText {
    let mut sanitized = Vec::with_capacity(output.byte_len());
    for (index, line) in output.as_bytes().split(|byte| *byte == b'\n').enumerate() {
        if index != 0 {
            sanitized.push(b'\n');
        }
        sanitized.extend_from_slice(utf8_sanitize(line).as_bytes());
    }
    RawText::from_bytes(sanitized)
}

pub(super) fn format_byte_width(value: &[u8]) -> usize {
    let mut width = 0;
    let mut index = 0;
    while index < value.len() {
        let (length, columns, _) = next_format_unit(value, index);
        index += length;
        width += columns;
    }
    width
}

pub(super) fn append_padded(output: &mut Vec<u8>, value: &[u8], width: isize) {
    if width == 0 {
        output.extend_from_slice(value);
        return;
    }
    let padding = width
        .unsigned_abs()
        .saturating_sub(format_byte_width(value));
    if width < 0 {
        output.extend(std::iter::repeat_n(b' ', padding));
    }
    output.extend_from_slice(value);
    if width > 0 {
        output.extend(std::iter::repeat_n(b' ', padding));
    }
}

fn format_trim_left(bytes: &[u8], limit: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut width = 0usize;
    let mut position = 0usize;
    while position < bytes.len() && width < limit {
        if bytes[position] == b'#' {
            let (end, leading) = leading_hashes(bytes, position);
            let leading = leading.min(limit - width);
            if leading != 0 {
                if end == position + 1 {
                    out.push(b'#');
                } else {
                    out.extend(std::iter::repeat_n(b'#', 2 * leading));
                }
                width += leading;
            }
            position = end;
            if bytes.get(position) == Some(&b'#') {
                let Some(end) = find_measured_style_end(bytes, position + 2) else {
                    break;
                };
                out.extend_from_slice(&bytes[position..=end]);
                position = end + 1;
            }
            continue;
        }
        let (length, unit, kept) = next_format_unit(bytes, position);
        if kept && width + unit <= limit {
            out.extend_from_slice(&bytes[position..position + length]);
        }
        width += unit;
        position += length;
    }
    out
}

fn format_trim_right(bytes: &[u8], skip: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut width = 0usize;
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'#' {
            let (end, leading) = leading_hashes(bytes, position);
            let mut copy_width = leading;
            if width <= skip {
                copy_width = copy_width.saturating_sub(skip - width);
            }
            if copy_width != 0 {
                if end == position + 1 {
                    out.push(b'#');
                } else {
                    out.extend(std::iter::repeat_n(b'#', 2 * copy_width));
                }
            }
            width += leading;
            position = end;
            if bytes.get(position) == Some(&b'#') {
                let Some(end) = find_measured_style_end(bytes, position + 2) else {
                    break;
                };
                out.extend_from_slice(&bytes[position..=end]);
                position = end + 1;
            }
            continue;
        }
        let (length, unit, kept) = next_format_unit(bytes, position);
        if kept && width >= skip {
            out.extend_from_slice(&bytes[position..position + length]);
        }
        width += unit;
        position += length;
    }
    out
}

pub(super) fn truncate_value(value: &RawText, limit: isize, marker: Option<&str>) -> RawText {
    if limit == 0 {
        return value.clone();
    }
    let keep = limit.unsigned_abs();
    let bytes = value.as_bytes();
    let trimmed = if limit > 0 {
        format_trim_left(bytes, keep)
    } else {
        let total = styled_display_width(bytes);
        if total <= keep {
            return value.clone();
        }
        format_trim_right(bytes, total - keep)
    };
    if trimmed == bytes {
        return value.clone();
    }
    let mut output = Vec::with_capacity(trimmed.len() + marker.map_or(0, str::len));
    match (limit > 0, marker) {
        (true, Some(marker)) => {
            output.extend_from_slice(&trimmed);
            output.extend_from_slice(marker.as_bytes());
        }
        (false, Some(marker)) => {
            output.extend_from_slice(marker.as_bytes());
            output.extend_from_slice(&trimmed);
        }
        _ => output = trimmed,
    }
    RawText::from_bytes(output)
}
