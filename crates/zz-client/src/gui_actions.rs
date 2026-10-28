#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuiChoice {
    pub value: &'static str,
    pub title: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuiValues {
    Fixed(&'static [GuiChoice]),
    Listed,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GuiAction {
    pub id: &'static str,
    pub title: &'static str,
    pub aliases: &'static [&'static str],
    pub values: GuiValues,
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
pub const THEME: &str = "interface-theme";
pub const SIDEBAR_LAYOUT: &str = "flat-sidebar";

pub const GUI_ACTIONS: [GuiAction; 5] = [
    GuiAction {
        id: TITLEBAR_MODE,
        title: "Titlebar mode",
        aliases: &["toggle sidebar", "hide sidebar", "show sidebar"],
        values: GuiValues::Fixed(&[
            GuiChoice {
                value: "sidebar",
                title: "Sidebar",
            },
            GuiChoice {
                value: "titlebar",
                title: "Title bar",
            },
        ]),
        config_key: None,
        preview: false,
    },
    GuiAction {
        id: PANE_GAPS,
        title: "Pane gaps",
        aliases: &["gaps", "frames"],
        values: GuiValues::Fixed(ON_OFF),
        config_key: Some(PANE_GAPS),
        preview: false,
    },
    GuiAction {
        id: WINDOW_BLUR,
        title: "Window blur",
        aliases: &["blur", "translucent"],
        values: GuiValues::Fixed(ON_OFF),
        config_key: Some(WINDOW_BLUR),
        preview: false,
    },
    GuiAction {
        id: THEME,
        title: "Theme",
        aliases: &["color theme", "look"],
        values: GuiValues::Listed,
        config_key: Some(THEME),
        preview: true,
    },
    GuiAction {
        id: SIDEBAR_LAYOUT,
        title: "Sidebar layout",
        aliases: &["list", "tree", "flat"],
        values: GuiValues::Fixed(&[
            GuiChoice {
                value: "true",
                title: "List",
            },
            GuiChoice {
                value: "false",
                title: "Tree",
            },
        ]),
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
        let GuiValues::Fixed(choices) = action.values else {
            return None;
        };
        Some(Self {
            action,
            current: current.to_owned(),
            options: choices
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
    pub fn listed(id: &str, current: &str, options: Vec<GuiOption>) -> Option<Self> {
        let action = gui_action(id).filter(|action| action.values == GuiValues::Listed)?;
        Some(Self {
            action,
            current: current.to_owned(),
            options,
        })
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
    fn fixed_actions_take_two_values_or_more() {
        for action in &GUI_ACTIONS {
            if let GuiValues::Fixed(choices) = action.values {
                assert!(choices.len() >= 2, "{}", action.id);
                let values: BTreeSet<_> = choices.iter().map(|choice| choice.value).collect();
                assert_eq!(values.len(), choices.len(), "{}", action.id);
            }
        }
    }

    #[test]
    fn a_two_valued_action_flips_and_a_listed_one_does_not() {
        let gaps = GuiOffer::toggle(PANE_GAPS, true).unwrap();
        assert_eq!(gaps.current_title(), "On");
        assert_eq!(gaps.flipped(), Some("false"));
        assert_eq!(
            GuiOffer::fixed(TITLEBAR_MODE, "titlebar")
                .unwrap()
                .flipped(),
            Some("sidebar")
        );
        let options = ["default", "macos", "yaru"]
            .map(|id| GuiOption {
                value: id.to_owned(),
                title: id.to_owned(),
            })
            .to_vec();
        let theme = GuiOffer::listed(THEME, "macos", options).unwrap();
        assert_eq!(theme.flipped(), None);
        assert!(GuiOffer::listed(PANE_GAPS, "true", Vec::new()).is_none());
        assert!(GuiOffer::fixed(THEME, "default").is_none());
    }
}
