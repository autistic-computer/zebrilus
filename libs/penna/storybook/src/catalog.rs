use std::sync::OnceLock;

use ala::App;
use penna::IconName;

use crate::{Knobs, Section, Story};

pub struct Catalog {
    pub title: &'static str,
    pub name: &'static str,
    pub storage: &'static str,
    pub register: fn(),
    pub init: fn(&mut App),
    pub logo: &'static str,
    pub plates: &'static [Section],
    pub stories: &'static [&'static [Story]],
    pub knobs: &'static [Knob],
    pub apply: fn(&Knobs, &mut App),
    pub floating: &'static [&'static [&'static str]],
}

pub struct Knob {
    pub name: &'static str,
    pub label: &'static str,
    pub section: &'static str,
    pub control: Control,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    Check(bool),
    Range {
        min: f32,
        max: f32,
        step: f32,
        value: f32,
    },
}

impl Control {
    pub fn value(self) -> f32 {
        match self {
            Self::Check(on) => f32::from(u8::from(on)),
            Self::Range { value, .. } => value,
        }
    }
}

pub static PENNA: Catalog = Catalog {
    title: "penna storybook",
    name: "penna",
    storage: "penna-storybook",
    register: || {},
    init: penna::init,
    logo: IconName::Pencil.path(),
    plates: crate::creator::PLATES,
    stories: &[crate::FOUNDATION, crate::KIT],
    knobs: &[],
    apply: |_, _| {},
    floating: &[crate::backdrop::KIT_FLOATING],
};

static INSTALLED: OnceLock<&'static Catalog> = OnceLock::new();

pub fn install(catalog: &'static Catalog) {
    let installed = *INSTALLED.get_or_init(|| {
        (catalog.register)();
        catalog
    });
    assert!(
        std::ptr::eq(installed, catalog),
        "the storybook already runs the {} catalog",
        installed.name
    );
}

pub fn catalog() -> &'static Catalog {
    INSTALLED.get().copied().unwrap_or(&PENNA)
}

impl Catalog {
    pub fn stories(&self) -> impl Iterator<Item = Story> {
        let creator = (!self.plates.is_empty()).then(|| crate::creator::story(self.plates));
        creator.into_iter().chain(
            self.stories
                .iter()
                .flat_map(|stories| stories.iter().copied()),
        )
    }

    pub fn floats(&self, story: &str, section: &str) -> bool {
        self.floating
            .iter()
            .flat_map(|entries| entries.iter())
            .any(|entry| match entry.split_once('/') {
                Some((entry_story, entry_section)) => {
                    entry_story == story && entry_section == section
                }
                None => *entry == story,
            })
    }

    pub fn problems(&self) -> Vec<String> {
        let url_safe = |id: &str| id.chars().all(|c| c.is_ascii_lowercase() || c == '-');
        let stories = self.stories().collect::<Vec<_>>();
        let mut problems = Vec::new();
        for (index, story) in stories.iter().enumerate() {
            if !url_safe(story.id) {
                problems.push(format!(
                    "story id {} is not lowercase with hyphens",
                    story.id
                ));
            }
            if stories[..index].iter().any(|other| other.id == story.id) {
                problems.push(format!("duplicate story {}", story.id));
            }
            for (position, section) in story.sections.iter().enumerate() {
                if !url_safe(section.id) {
                    problems.push(format!(
                        "section id {}/{} is not lowercase with hyphens",
                        story.id, section.id
                    ));
                }
                if story.sections[..position]
                    .iter()
                    .any(|other| other.id == section.id)
                {
                    problems.push(format!("duplicate section {}/{}", story.id, section.id));
                }
            }
        }
        for entry in self.floating.iter().flat_map(|entries| entries.iter()) {
            let (story, section) = entry.split_once('/').unwrap_or((entry, ""));
            match stories.iter().find(|candidate| candidate.id == story) {
                None => problems.push(format!("floating entry {entry} names no story")),
                Some(story)
                    if !section.is_empty()
                        && story.sections.iter().all(|other| other.id != section) =>
                {
                    problems.push(format!("floating entry {entry} names no section"));
                }
                Some(_) => {}
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_penna_catalog_has_no_problems() {
        assert_eq!(PENNA.problems(), Vec::<String>::new());
    }

    #[test]
    fn foundation_comes_before_the_kit() {
        let groups = PENNA.stories().map(|story| story.group).collect::<Vec<_>>();
        let first_kit = groups.iter().position(|group| *group == "Kit").unwrap();
        assert!(
            groups[..first_kit]
                .iter()
                .all(|group| *group == "Foundation")
        );
        assert!(groups[first_kit..].iter().all(|group| *group == "Kit"));
    }

    #[test]
    fn the_creator_opens_the_page_with_the_catalogs_plates() {
        let creator = PENNA.stories().next().unwrap();
        assert_eq!(creator.id, crate::creator::ID);
        assert_eq!(creator.group, "Foundation");
        let plates = creator
            .sections
            .iter()
            .map(|plate| plate.id)
            .collect::<Vec<_>>();
        assert_eq!(
            plates,
            ["window", "controls", "floating", "content", "phone"]
        );
        let without = Catalog {
            plates: &[],
            ..PENNA
        };
        assert!(
            without
                .stories()
                .all(|story| story.id != crate::creator::ID)
        );
    }

    #[test]
    fn a_floating_entry_names_a_story_or_one_of_its_sections() {
        assert!(PENNA.floats("dialogs", "anything"));
        assert!(PENNA.floats("menus", "popup-menu"));
        assert!(!PENNA.floats("menus", "triggers"));
        assert!(!PENNA.floats("buttons", "variants"));
    }
}
