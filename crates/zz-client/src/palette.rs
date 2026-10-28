use std::ops::Range;

use zz_protocol::{CommandSpec, CommandValueKind};

#[must_use]
pub fn fuzzy_match(query: &str, label: &str) -> Option<(usize, Vec<Range<usize>>)> {
    let mut needle = query
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .peekable();
    if needle.peek().is_none() {
        return Some((0, Vec::new()));
    }
    let mut hits = Vec::new();
    let mut first = None;
    let mut previous = 0;
    let mut gaps = 0;
    for (position, (byte, character)) in label.char_indices().enumerate() {
        if character
            .to_lowercase()
            .any(|ch| Some(&ch) == needle.peek())
        {
            needle.next();
            if first.is_none() {
                first = Some(position);
            } else {
                gaps += position - previous - 1;
            }
            previous = position;
            hits.push(byte..byte + character.len_utf8());
            if needle.peek().is_none() {
                return Some((first.unwrap_or_default() + 2 * gaps, hits));
            }
        }
    }
    None
}

#[must_use]
pub fn target_kind(spec: &CommandSpec) -> Option<CommandValueKind> {
    if !matches!(
        spec.name,
        "join-pane"
            | "move-pane"
            | "swap-pane"
            | "swap-window"
            | "move-window"
            | "link-window"
            | "unlink-window"
            | "kill-pane"
            | "kill-window"
            | "select-pane"
            | "select-window"
            | "resize-pane"
            | "capture-pane"
            | "respawn-pane"
            | "respawn-window"
    ) {
        return None;
    }
    if spec.name == "join-pane" {
        return Some(CommandValueKind::Window);
    }
    spec.option("-t")
        .filter(|option| option.completable && !option.unsupported)
        .and_then(|option| option.value)
        .filter(|kind| matches!(kind, CommandValueKind::Window | CommandValueKind::Pane))
}

#[must_use]
pub fn needs_arguments(spec: &CommandSpec) -> bool {
    let mut optional_depth = 0_u32;
    spec.usage.chars().any(|character| match character {
        '[' => {
            optional_depth += 1;
            false
        }
        ']' => {
            optional_depth = optional_depth.saturating_sub(1);
            false
        }
        character => optional_depth == 0 && !character.is_whitespace(),
    })
}

#[must_use]
pub fn has_command_arguments(input: &str) -> bool {
    input
        .find(char::is_whitespace)
        .is_some_and(|index| zz_protocol::catalog_command_spec(&input[..index]).is_some())
}

#[must_use]
pub fn count(value: usize, noun: &str) -> String {
    format!("{value} {noun}{}", if value == 1 { "" } else { "s" })
}

#[must_use]
pub fn tmux_keystroke_source(spelling: &str) -> Option<(String, String)> {
    let key = crate::ChromeKey::parse(spelling)?;
    let base = match key.base.as_str() {
        " " | "Space" => "space",
        "Enter" => "enter",
        "Escape" => "escape",
        "Tab" => "tab",
        "BSpace" => "backspace",
        "DC" => "delete",
        "Up" => "up",
        "Down" => "down",
        "Left" => "left",
        "Right" => "right",
        "Home" => "home",
        "End" => "end",
        "PPage" | "PageUp" => "pageup",
        "NPage" | "PageDown" => "pagedown",
        "-" => "minus",
        other if other.chars().count() == 1 => other,
        _ => return None,
    };
    let mut source = String::new();
    if key.command {
        source.push_str("cmd-");
    }
    if key.control {
        source.push_str("ctrl-");
    }
    if key.alt {
        source.push_str("alt-");
    }
    let mut characters = base.chars();
    match (characters.next(), characters.next()) {
        (Some(character), None) if character.is_ascii_uppercase() => {
            source.push_str("shift-");
            source.extend(character.to_lowercase());
        }
        _ => {
            if key.shift {
                source.push_str("shift-");
            }
            source.push_str(base);
        }
    }
    Some((source, base.to_owned()))
}
