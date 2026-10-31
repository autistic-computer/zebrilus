mod stories;

use ala::{App, px};
use penna_storybook::{Catalog, Control, Knob, Knobs};

pub(crate) use penna_storybook::{backdrop, knobs, story};

pub static CATALOG: Catalog = Catalog {
    title: "zz storybook",
    name: "zz",
    storage: "zz-storybook",
    register: zz_ui::register,
    init: zz_ui::init,
    logo: zz_ui::ZZ_LOGO,
    plates: stories::theme_creator::PLATES,
    stories: &[
        penna_storybook::FOUNDATION,
        stories::agent::STORIES,
        stories::workspace::STORIES,
        stories::console::STORIES,
        penna_storybook::KIT,
    ],
    knobs: KNOBS,
    apply,
    floating: &[backdrop::KIT_FLOATING, FLOATING],
};

const KNOBS: &[Knob] = &[
    Knob {
        name: "pane-gaps",
        label: "Gaps",
        section: "Panes",
        control: Control::Check(false),
    },
    Knob {
        name: "pane-margin",
        label: "Margin",
        section: "Panes",
        control: Control::Range {
            min: 0.0,
            max: 32.0,
            step: 0.5,
            value: 6.0,
        },
    },
    Knob {
        name: "pip-opacity",
        label: "PiP opacity",
        section: "Panes",
        control: Control::Range {
            min: 0.0,
            max: 1.0,
            step: 0.05,
            value: 0.8,
        },
    },
    Knob {
        name: "pane-inactive-opacity",
        label: "Inactive opacity",
        section: "Panes",
        control: Control::Range {
            min: 0.0,
            max: 1.0,
            step: 0.05,
            value: 0.7,
        },
    },
    Knob {
        name: "pane-glow",
        label: "Selected edge",
        section: "Panes",
        control: Control::Range {
            min: 0.0,
            max: 2.0,
            step: 0.1,
            value: 1.0,
        },
    },
    Knob {
        name: "program-colors",
        label: "Program colors",
        section: "Preview",
        control: Control::Check(true),
    },
];

const FLOATING: &[&str] = &[
    "floating-menus",
    "feedback-dialogs",
    "which-key",
    "command-palette/surface",
    "command-palette/workspace",
    "command-palette/search",
    "command-palette/offline",
    "command-palette/command-mode",
    "command-palette/command-query",
    "command-palette/no-matches",
    "daemon-prompts",
    "chooser",
    "path-picker",
    "phone/hud",
    "phone/keys",
    "panes/floating",
    "panes/display-panes",
    "browser/omnibox",
    "browser/action-menu",
    "browser/site-menu",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneKnobs {
    pub gaps: bool,
    pub margin: f32,
    pub pip: f32,
    pub inactive: f32,
    pub glow: f32,
}

impl PaneKnobs {
    fn of(knobs: &Knobs) -> Self {
        Self {
            gaps: knobs.flag("pane-gaps"),
            margin: knobs.value("pane-margin"),
            pip: knobs.value("pip-opacity"),
            inactive: knobs.value("pane-inactive-opacity"),
            glow: knobs.value("pane-glow"),
        }
    }
}

pub(crate) fn pane() -> PaneKnobs {
    PaneKnobs::of(&knobs())
}

fn apply(knobs: &Knobs, cx: &mut App) {
    let pane = PaneKnobs::of(knobs);
    cx.set_global(zz_ui::program::ProgramColors(knobs.flag("program-colors")));
    cx.set_global(zz_ui::pip::PipOpacity(pane.pip));
    cx.set_global(zz_ui::pane_look::PaneLook {
        frames: pane.gaps,
        glow_strength: pane.glow,
    });
    penna::Theme::global_mut(cx).pane_margin = px(if pane.gaps { pane.margin } else { 0.0 });
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn run() -> Result<(), wasm_bindgen::JsValue> {
    penna_storybook::start(&CATALOG, zz_ui::Assets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use penna::interface_theme::InterfaceTheme;

    #[test]
    fn the_catalog_has_no_problems() {
        penna_storybook::install(&CATALOG);
        assert_eq!(CATALOG.problems(), Vec::<String>::new());
    }

    #[test]
    fn the_theme_creator_comes_first_and_the_kit_keeps_its_order() {
        penna_storybook::install(&CATALOG);
        let ids = CATALOG.stories().map(|story| story.id).collect::<Vec<_>>();
        assert_eq!(ids[0], penna_storybook::creator::ID);
        let kit = penna_storybook::KIT
            .iter()
            .map(|story| story.id)
            .collect::<Vec<_>>();
        assert!(ids.ends_with(&kit));
        let foundation = penna_storybook::FOUNDATION
            .iter()
            .map(|story| story.id)
            .collect::<Vec<_>>();
        assert_eq!(ids[1..=foundation.len()], foundation[..]);
    }

    #[test]
    fn pane_and_program_knobs_parse_and_ignore_the_rest() {
        penna_storybook::install(&CATALOG);
        let defaults = PaneKnobs::of(&Knobs::parse("pane-glass=on").unwrap());
        assert_eq!(
            defaults,
            PaneKnobs {
                gaps: false,
                margin: 6.0,
                pip: 0.8,
                inactive: 0.7,
                glow: 1.0,
            }
        );
        let knobs =
            Knobs::parse("pane-gaps=1&pane-margin=20&pane-inactive-opacity=0.4&program-colors=0")
                .unwrap();
        let pane = PaneKnobs::of(&knobs);
        assert!(pane.gaps);
        assert_eq!(pane.margin, 20.0);
        assert_eq!(pane.inactive, 0.4);
        assert!(!knobs.flag("program-colors"));
        assert!(Knobs::parse("pane-margin=wide").is_err());
    }

    #[test]
    fn every_zz_role_inherits_what_the_theme_resolves() {
        penna_storybook::install(&CATALOG);
        let themes = zz_ui::interface_theme::builtin_themes()
            .iter()
            .map(|(id, theme)| ((*id).to_owned(), theme.clone()))
            .collect::<Vec<(String, InterfaceTheme)>>();
        for (id, theme) in themes {
            let mut kit = penna::Theme::default();
            theme.apply_roles(&mut kit);
            theme.materials.apply(&mut kit);
            for role in zz_ui::roles::all() {
                let (mut style, _) = knobs::inherited(role, &theme.roles, &theme.materials);
                if let Some(patch) = theme.roles.get(role.id) {
                    patch.apply(&mut style);
                }
                assert_eq!(style, kit.style(role), "{id} {}", role.id);
            }
        }
    }
}
