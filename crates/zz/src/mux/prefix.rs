use ala::Keystroke;
use zz_client::ChromeKey;
pub(crate) use zz_client::prefix_binding::{binds_input, is_all_keys_input};
use zz_protocol::{
    CommandInvocation, KeyBindingSnapshot, TmuxOption, command_spec, parse_tmux_command_options,
};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers as TerminalModifiers};
use zz_ui::pane::{PaneActionKeys, TerminalPaneAction};

pub(crate) fn display_keystroke(canonical: &str) -> Option<Keystroke> {
    let key = ChromeKey::parse(canonical)?;
    Keystroke::parse(&crate::keymap::ala_source(&key)?).ok()
}

pub(crate) fn split_hint_matches(command: &CommandInvocation, horizontal: bool) -> bool {
    if !matches!(command.name.as_str(), "split-window" | "splitw") {
        return false;
    }
    let Some(spec) = command_spec(&command.name) else {
        return false;
    };
    let Ok(parsed) = parse_tmux_command_options(spec, command) else {
        return false;
    };
    let kind = parsed
        .options
        .iter()
        .rev()
        .find_map(|option| match option {
            TmuxOption::Value("--kind", kind) => Some(*kind),
            _ => None,
        })
        .unwrap_or("terminal");
    matches!(kind, "terminal" | "picker")
        && parsed.options.contains(&TmuxOption::Flag("-h")) == horizontal
}

fn runs(command: &CommandInvocation, name: &str, flag: Option<&str>) -> bool {
    let runs_words = |mut words: std::str::SplitWhitespace<'_>| {
        words.next() == Some(name) && flag.is_none_or(|flag| words.any(|word| word == flag))
    };
    let direct =
        command.name == name && flag.is_none_or(|flag| command.args.iter().any(|arg| *arg == flag));
    direct
        || command.name == "if-shell"
            && command.args.iter().any(|arg| {
                arg.trim()
                    .strip_prefix('{')
                    .and_then(|text| text.strip_suffix('}'))
                    .is_some_and(|inner| runs_words(inner.split_whitespace()))
            })
}

type PaneActionBinding = (
    TerminalPaneAction,
    &'static str,
    fn(&CommandInvocation) -> bool,
);

const PANE_ACTION_BINDINGS: [PaneActionBinding; 6] = [
    (TerminalPaneAction::SplitBottom, "\"", |command| {
        split_hint_matches(command, false)
    }),
    (TerminalPaneAction::SplitRight, "%", |command| {
        split_hint_matches(command, true)
    }),
    (TerminalPaneAction::Float, "@", |command| {
        runs(command, "break-pane", Some("-W"))
    }),
    (TerminalPaneAction::Tile, "@", |command| {
        runs(command, "join-pane", None)
    }),
    (TerminalPaneAction::Zoom, "z", |command| {
        runs(command, "resize-pane", Some("-Z"))
    }),
    (TerminalPaneAction::Close, "x", |command| {
        runs(command, "kill-pane", None)
            || command.name == "confirm-before"
                && command
                    .args
                    .last()
                    .is_some_and(|arg| arg.split_whitespace().next() == Some("kill-pane"))
    }),
];

pub(crate) fn bound_key(
    bindings: &[KeyBindingSnapshot],
    stock: &str,
    matches: fn(&CommandInvocation) -> bool,
) -> Option<Keystroke> {
    let stock_key = || display_keystroke(stock);
    if bindings.is_empty() {
        return stock_key();
    }
    let binding = bindings
        .iter()
        .filter(|binding| binding.commands.first().is_some_and(matches))
        .min_by_key(|binding| binding.key == stock)?;
    display_keystroke(&binding.key).or_else(stock_key)
}

pub(crate) fn pane_action_keys(
    prefix: Option<&str>,
    bindings: &[KeyBindingSnapshot],
) -> PaneActionKeys {
    let Some(prefix) = prefix.and_then(display_keystroke) else {
        return PaneActionKeys::default();
    };
    PaneActionKeys::new(
        PANE_ACTION_BINDINGS
            .iter()
            .filter_map(|(action, stock, matches)| {
                bound_key(bindings, stock, *matches).map(|key| (*action, vec![prefix.clone(), key]))
            }),
    )
}

pub(crate) fn terminal_key_input(keystroke: &Keystroke, action: KeyAction) -> KeyInput {
    let key = terminal_key(&keystroke.key);
    let character = match key {
        KeyCode::Character(character) => Some(character),
        _ => None,
    };
    KeyInput {
        action,
        key,
        modifiers: TerminalModifiers::new(
            keystroke.modifiers.shift,
            keystroke.modifiers.control,
            keystroke.modifiers.alt,
            keystroke.modifiers.platform,
        ),
        text: keystroke
            .key_char
            .clone()
            .or_else(|| character.map(|value| value.to_string()))
            .map(String::into_boxed_str),
        unshifted_codepoint: character,
    }
}

fn terminal_key(key: &str) -> KeyCode {
    match key {
        "space" => KeyCode::Character(' '),
        "backspace" => KeyCode::Backspace,
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "escape" => KeyCode::Escape,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "up" => KeyCode::ArrowUp,
        "down" => KeyCode::ArrowDown,
        "left" => KeyCode::ArrowLeft,
        "right" => KeyCode::ArrowRight,
        value => {
            let mut characters = value.chars();
            if let (Some(character), None) = (characters.next(), characters.next()) {
                KeyCode::Character(character)
            } else {
                value
                    .strip_prefix('f')
                    .and_then(|number| number.parse::<u8>().ok())
                    .map_or(KeyCode::Unidentified, KeyCode::Function)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zz_protocol::{Binding, KeyTables};

    #[test]
    fn pane_buttons_show_the_prefix_and_the_key_bound_to_their_command() {
        let mut tables = KeyTables::default();
        tables.bind(
            "prefix",
            "|",
            Binding {
                commands: vec![CommandInvocation::new("split-window", ["-h"])],
                repeat: false,
                note: None,
            },
        );
        let prefix = tables
            .snapshot()
            .into_iter()
            .find(|table| table.name == "prefix")
            .expect("the prefix table")
            .bindings;
        let keys = pane_action_keys(Some("C-a"), &prefix);
        let spelled = |action| {
            keys.of(action)
                .iter()
                .map(ala::Keystroke::unparse)
                .collect::<Vec<_>>()
        };
        assert_eq!(spelled(TerminalPaneAction::SplitRight), ["ctrl-a", "|"]);
        assert_eq!(spelled(TerminalPaneAction::SplitBottom), ["ctrl-a", "\""]);
        assert_eq!(spelled(TerminalPaneAction::Float), ["ctrl-a", "@"]);
        assert_eq!(spelled(TerminalPaneAction::Tile), ["ctrl-a", "@"]);
        assert_eq!(spelled(TerminalPaneAction::Zoom), ["ctrl-a", "z"]);
        assert_eq!(spelled(TerminalPaneAction::Close), ["ctrl-a", "x"]);
        assert!(
            pane_action_keys(None, &prefix)
                .of(TerminalPaneAction::Zoom)
                .is_empty()
        );
    }
}
