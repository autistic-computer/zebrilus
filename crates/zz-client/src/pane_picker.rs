//! A freshly split pane asking what it should become.

use zz_protocol::{CommandInvocation, PaneId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneChoice {
    pub title: &'static str,
    pub kind: &'static str,
    pub shortcut: &'static str,
}

const CHOICES: &[PaneChoice] = &[
    PaneChoice {
        title: "Terminal",
        kind: "terminal",
        shortcut: "t",
    },
    PaneChoice {
        title: "Browser",
        kind: "browser",
        shortcut: "b",
    },
];

/// What a picker offers: a browser only where the client can draw one.
#[must_use]
pub fn pane_choices(browser: bool) -> &'static [PaneChoice] {
    if browser { CHOICES } else { &CHOICES[..1] }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanePickerKey {
    Select(usize),
    Choose(usize),
    Close,
    Ignored,
}

/// What a key pressed in a picker of `choices` with `selected` lit does. A
/// key with Control, Alt or the platform key held is not the picker's.
#[must_use]
pub fn pane_picker_key(
    choices: &[PaneChoice],
    selected: usize,
    key: &str,
    modified: bool,
) -> PanePickerKey {
    if modified || choices.is_empty() {
        return PanePickerKey::Ignored;
    }
    match key {
        "down" | "j" => PanePickerKey::Select((selected + 1) % choices.len()),
        "up" | "k" => PanePickerKey::Select((selected + choices.len() - 1) % choices.len()),
        "enter" => PanePickerKey::Choose(selected),
        "escape" => PanePickerKey::Close,
        shortcut => choices
            .iter()
            .position(|choice| choice.shortcut == shortcut)
            .map_or(PanePickerKey::Ignored, PanePickerKey::Choose),
    }
}

#[must_use]
pub fn pane_kind_command(pane: PaneId, choice: &PaneChoice) -> CommandInvocation {
    CommandInvocation::new(
        "select-pane-kind",
        ["-t".to_owned(), pane.to_string(), choice.kind.to_owned()],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_cycle_choose_and_close() {
        let choices = pane_choices(true);
        assert_eq!(
            pane_picker_key(choices, 0, "up", false),
            PanePickerKey::Select(1)
        );
        assert_eq!(
            pane_picker_key(choices, 1, "j", false),
            PanePickerKey::Select(0)
        );
        assert_eq!(
            pane_picker_key(choices, 1, "enter", false),
            PanePickerKey::Choose(1)
        );
        assert_eq!(
            pane_picker_key(choices, 0, "b", false),
            PanePickerKey::Choose(1)
        );
        assert_eq!(
            pane_picker_key(choices, 0, "b", true),
            PanePickerKey::Ignored
        );
        assert_eq!(
            pane_picker_key(pane_choices(false), 0, "b", false),
            PanePickerKey::Ignored
        );
        assert_eq!(
            pane_picker_key(choices, 0, "escape", false),
            PanePickerKey::Close
        );
    }
}
