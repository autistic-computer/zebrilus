#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuiChoice {
    pub value: &'static str,
    pub title: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GuiAction {
    pub id: &'static str,
    pub title: &'static str,
    pub verb: &'static str,
    pub aliases: &'static [&'static str],
    pub values: &'static [GuiChoice],
    pub config_key: Option<&'static str>,
    pub preview: bool,
}

const ON_OFF: &[GuiChoice] = &[
    GuiChoice {
        value: "true",
        title: "On",
    },
    GuiChoice {
        value: "false",
        title: "Off",
    },
];

pub const TITLEBAR_MODE: &str = "titlebar-mode";
pub const PANE_GAPS: &str = "pane-gaps";
pub const WINDOW_BLUR: &str = "window-background-blur";
pub const ACCENT: &str = "chrome-accent";
pub const SIDEBAR_LAYOUT: &str = "flat-sidebar";

pub const GUI_ACTIONS: [GuiAction; 5] = [
    GuiAction {
        id: TITLEBAR_MODE,
        title: "Titlebar mode",
        verb: "titlebar mode",
        aliases: &["toggle sidebar", "hide sidebar", "show sidebar"],
        values: &[
            GuiChoice {
                value: "sidebar",
                title: "Sidebar",
            },
            GuiChoice {
                value: "titlebar",
                title: "Title bar",
            },
        ],
        config_key: None,
        preview: false,
    },
    GuiAction {
        id: PANE_GAPS,
        title: "Pane gaps",
        verb: "pane gaps",
        aliases: &["gaps", "frames"],
        values: ON_OFF,
        config_key: Some(PANE_GAPS),
        preview: false,
    },
    GuiAction {
        id: WINDOW_BLUR,
        title: "Window blur",
        verb: "window blur",
        aliases: &["blur", "translucent"],
        values: ON_OFF,
        config_key: Some(WINDOW_BLUR),
        preview: false,
    },
    GuiAction {
        id: ACCENT,
        title: "Accent",
        verb: "select accent",
        aliases: &["accent color", "color"],
        values: &[
            GuiChoice {
                value: "ink",
                title: "Ink",
            },
            GuiChoice {
                value: "blue",
                title: "Blue",
            },
            GuiChoice {
                value: "purple",
                title: "Purple",
            },
            GuiChoice {
                value: "pink",
                title: "Pink",
            },
            GuiChoice {
                value: "red",
                title: "Red",
            },
            GuiChoice {
                value: "orange",
                title: "Orange",
            },
            GuiChoice {
                value: "yellow",
                title: "Yellow",
            },
            GuiChoice {
                value: "green",
                title: "Green",
            },
            GuiChoice {
                value: "graphite",
                title: "Graphite",
            },
        ],
        config_key: Some(ACCENT),
        preview: true,
    },
    GuiAction {
        id: SIDEBAR_LAYOUT,
        title: "Sidebar layout",
        verb: "sidebar layout",
        aliases: &["list", "tree", "flat"],
        values: &[
            GuiChoice {
                value: "true",
                title: "List",
            },
            GuiChoice {
                value: "false",
                title: "Tree",
            },
        ],
        config_key: Some(SIDEBAR_LAYOUT),
        preview: false,
    },
];

#[must_use]
pub fn gui_action(id: &str) -> Option<&'static GuiAction> {
    GUI_ACTIONS.iter().find(|action| action.id == id)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuiOption {
    pub value: String,
    pub title: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuiOffer {
    pub action: &'static GuiAction,
    pub current: String,
    pub options: Vec<GuiOption>,
}

impl GuiOffer {
    #[must_use]
    pub fn fixed(id: &str, current: &str) -> Option<Self> {
        let action = gui_action(id)?;
        Some(Self {
            action,
            current: current.to_owned(),
            options: action
                .values
                .iter()
                .map(|choice| GuiOption {
                    value: choice.value.to_owned(),
                    title: choice.title.to_owned(),
                })
                .collect(),
        })
    }
    #[must_use]
    pub fn toggle(id: &str, on: bool) -> Option<Self> {
        Self::fixed(id, if on { "true" } else { "false" })
    }

    #[must_use]
    pub fn current_title(&self) -> &str {
        self.options
            .iter()
            .find(|option| option.value == self.current)
            .map_or(self.current.as_str(), |option| option.title.as_str())
    }
    #[must_use]
    pub fn flipped(&self) -> Option<&str> {
        let [first, second] = self.options.as_slice() else {
            return None;
        };
        Some(if first.value == self.current {
            &second.value
        } else {
            &first.value
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn ids_titles_and_aliases_are_distinct() {
        let mut names = BTreeSet::new();
        for action in &GUI_ACTIONS {
            assert!(names.insert(action.id), "{} twice", action.id);
            assert!(
                names.insert(action.title),
                "{} names two actions",
                action.title
            );
            for alias in action.aliases {
                assert!(
                    GUI_ACTIONS
                        .iter()
                        .all(|other| other.title.to_lowercase() != *alias),
                    "{alias} is a title"
                );
            }
        }
    }

    #[test]
    fn actions_take_two_values_or_more() {
        for action in &GUI_ACTIONS {
            assert!(action.values.len() >= 2, "{}", action.id);
            let values: BTreeSet<_> = action.values.iter().map(|choice| choice.value).collect();
            assert_eq!(values.len(), action.values.len(), "{}", action.id);
        }
    }

    #[test]
    fn a_two_valued_action_flips_and_a_longer_one_does_not() {
        let gaps = GuiOffer::toggle(PANE_GAPS, true).unwrap();
        assert_eq!(gaps.current_title(), "On");
        assert_eq!(gaps.flipped(), Some("false"));
        assert_eq!(
            GuiOffer::fixed(TITLEBAR_MODE, "titlebar")
                .unwrap()
                .flipped(),
            Some("sidebar")
        );
        let accent = GuiOffer::fixed(ACCENT, "ink").unwrap();
        assert_eq!(accent.current_title(), "Ink");
        assert_eq!(accent.flipped(), None);
    }
}
