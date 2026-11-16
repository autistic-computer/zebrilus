use zz_protocol::{
    Binding, CommandInvocation, KeyBindingSnapshot, KeyTables, TmuxOption, command_spec,
    parse_tmux_command_options,
};
use zz_terminal::KeyInput;

pub const DEFAULT_WHICH_KEY_DELAY: f32 = 400.0;
pub const MAX_WHICH_KEY_DELAY: f32 = 2000.0;

#[must_use]
pub fn which_key_delay(milliseconds: f32) -> Option<std::time::Duration> {
    let milliseconds = milliseconds.round();
    (milliseconds.is_finite() && milliseconds > 0.0)
        .then(|| std::time::Duration::from_secs_f64(f64::from(milliseconds) / 1000.0))
}

pub struct PrefixHint {
    pub key: &'static str,
    pub label: &'static str,
    pub matches: fn(&CommandInvocation) -> bool,
}

pub const PREFIX_HINTS: [PrefixHint; 5] = [
    PrefixHint {
        key: "c",
        label: "New window",
        matches: |command| command.name == "new-window",
    },
    PrefixHint {
        key: "%",
        label: "Split right",
        matches: |command| split_hint_matches(command, true),
    },
    PrefixHint {
        key: "\"",
        label: "Split down",
        matches: |command| split_hint_matches(command, false),
    },
    PrefixHint {
        key: "s",
        label: "Sessions and windows",
        matches: |command| matches!(command.name.as_str(), "focus-sidebar" | "choose-tree"),
    },
    PrefixHint {
        key: "?",
        label: "Every key binding",
        matches: |command| command.name == "list-keys",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneKey {
    SplitBottom,
    SplitRight,
    Float,
    Tile,
    Zoom,
    Close,
}

type PaneKeyBinding = (PaneKey, &'static str, fn(&CommandInvocation) -> bool);

const PANE_KEYS: [PaneKeyBinding; 6] = [
    (PaneKey::SplitBottom, "\"", |command| {
        split_hint_matches(command, false)
    }),
    (PaneKey::SplitRight, "%", |command| {
        split_hint_matches(command, true)
    }),
    (PaneKey::Float, "@", |command| {
        runs(command, "break-pane", Some("-W"))
    }),
    (PaneKey::Tile, "@", |command| {
        runs(command, "join-pane", None)
    }),
    (PaneKey::Zoom, "z", |command| {
        runs(command, "resize-pane", Some("-Z"))
    }),
    (PaneKey::Close, "x", |command| {
        runs(command, "kill-pane", None)
            || command.name == "confirm-before"
                && command
                    .args
                    .last()
                    .is_some_and(|arg| arg.split_whitespace().next() == Some("kill-pane"))
    }),
];

#[must_use]
pub fn split_hint_matches(command: &CommandInvocation, horizontal: bool) -> bool {
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

pub fn bound_key<K>(
    bindings: &[KeyBindingSnapshot],
    stock: &str,
    matches: fn(&CommandInvocation) -> bool,
    display: impl Fn(&str) -> Option<K>,
) -> Option<K> {
    if bindings.is_empty() {
        return display(stock);
    }
    let binding = bindings
        .iter()
        .filter(|binding| binding.commands.first().is_some_and(matches))
        .min_by_key(|binding| binding.key == stock)?;
    display(&binding.key).or_else(|| display(stock))
}

pub fn pane_keys<K: Clone>(
    prefix: Option<&str>,
    bindings: &[KeyBindingSnapshot],
    display: impl Fn(&str) -> Option<K>,
) -> Vec<(PaneKey, [K; 2])> {
    let Some(prefix) = prefix.and_then(&display) else {
        return Vec::new();
    };
    PANE_KEYS
        .iter()
        .filter_map(|(key, stock, matches)| {
            bound_key(bindings, stock, *matches, &display)
                .map(|bound| (*key, [prefix.clone(), bound]))
        })
        .collect()
}

fn prefix_binding(bindings: &[KeyBindingSnapshot], input: &KeyInput) -> Option<Binding> {
    let mut tables = KeyTables::empty();
    for binding in bindings {
        tables.bind(
            "prefix",
            &binding.key,
            Binding {
                commands: binding.commands.clone(),
                repeat: binding.repeat,
                note: None,
            },
        );
    }
    tables.resolve_input("prefix", input).cloned()
}

#[must_use]
pub fn binds_input(bindings: &[KeyBindingSnapshot], input: &KeyInput) -> bool {
    prefix_binding(bindings, input).is_some()
}

#[must_use]
pub fn is_all_keys_input(bindings: &[KeyBindingSnapshot], input: &KeyInput) -> bool {
    prefix_binding(bindings, input)
        .is_some_and(|binding| crate::which_key::opens_all_keys(&binding.commands))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn spelled(key: &str) -> Option<String> {
        (!key.is_empty()).then(|| key.to_owned())
    }

    fn picker_split(key: &str) -> KeyBindingSnapshot {
        KeyBindingSnapshot {
            key: key.to_owned(),
            commands: vec![CommandInvocation::new(
                "split-window",
                ["--kind", "picker", "-h"],
            )],
            repeat: false,
            note: None,
        }
    }

    #[test]
    fn delay_rounds_to_milliseconds_and_zero_turns_it_off() {
        assert_eq!(which_key_delay(0.0), None);
        assert_eq!(which_key_delay(0.4), None);
        assert_eq!(which_key_delay(400.0), Some(Duration::from_millis(400)));
        assert_eq!(which_key_delay(249.6), Some(Duration::from_millis(250)));
        assert_eq!(which_key_delay(2000.0), Some(Duration::from_secs(2)));
    }

    #[test]
    fn split_hints_match_only_terminal_and_picker_kinds() {
        for name in ["split-window", "splitw"] {
            for kind in ["terminal", "picker", "browser", "agent"] {
                for (flag, horizontal) in [("-h", true), ("-v", false)] {
                    let command = CommandInvocation::new(name, ["--kind", kind, flag]);
                    assert_eq!(
                        split_hint_matches(&command, horizontal),
                        matches!(kind, "terminal" | "picker")
                    );
                    assert!(!split_hint_matches(&command, !horizontal));
                }
            }
            assert!(split_hint_matches(
                &CommandInvocation::new(name, ["-h"]),
                true
            ));
            assert!(split_hint_matches(
                &CommandInvocation::new(name, [] as [&str; 0]),
                false
            ));
        }
    }

    #[test]
    fn pane_keys_pair_the_prefix_with_the_key_bound_to_each_command() {
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
        let keys = pane_keys(Some("C-a"), &prefix, spelled);
        let of = |wanted| {
            keys.iter()
                .find(|(key, _)| *key == wanted)
                .map(|(_, keys)| keys.clone())
        };
        let pair = |key: &str| Some(["C-a".to_owned(), key.to_owned()]);
        assert_eq!(of(PaneKey::SplitRight), pair("|"));
        assert_eq!(of(PaneKey::SplitBottom), pair("\""));
        assert_eq!(of(PaneKey::Float), pair("@"));
        assert_eq!(of(PaneKey::Tile), pair("@"));
        assert_eq!(of(PaneKey::Zoom), pair("z"));
        assert_eq!(of(PaneKey::Close), pair("x"));
        assert!(pane_keys(None, &prefix, spelled).is_empty());
    }

    #[test]
    fn bound_key_prefers_yours_falls_back_to_stock_and_omits_unbound() {
        let split = PREFIX_HINTS[1].matches;
        let resolve = |bindings: &[KeyBindingSnapshot]| bound_key(bindings, "%", split, spelled);
        assert_eq!(
            resolve(&[picker_split("%"), picker_split("|")]),
            Some("|".to_owned())
        );
        assert_eq!(resolve(&[]), Some("%".to_owned()));
        assert_eq!(resolve(&[picker_split("")]), Some("%".to_owned()));
        let new_window = KeyBindingSnapshot {
            key: "c".to_owned(),
            commands: vec![CommandInvocation::new("new-window", [] as [&str; 0])],
            repeat: false,
            note: None,
        };
        assert_eq!(resolve(&[new_window]), None);
    }
}
