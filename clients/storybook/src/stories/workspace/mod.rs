use crate::story::Story;

pub(super) mod browser;
pub(super) mod fixtures;
pub(super) mod flat_sidebar;
mod navigation;
pub(super) mod panes;
mod settings;
pub(super) mod shell;
pub(super) mod status_bar;
pub(super) mod tree;

pub const STORIES: &[Story] = &[
    shell::STORY,
    panes::STORY,
    navigation::STORY,
    flat_sidebar::STORY,
    status_bar::STORY,
    browser::STORY,
    settings::STORY,
];
