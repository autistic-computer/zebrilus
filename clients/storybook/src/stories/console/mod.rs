use crate::story::Story;

pub(super) mod chooser;
mod dialogs;
mod menus;
pub(super) mod palette;
mod path_picker;
pub(super) mod phone;
mod terminal;
mod tmux_styles;

pub const STORIES: &[Story] = &[
    palette::STORY,
    palette::PROMPTS,
    chooser::STORY,
    path_picker::STORY,
    menus::STORY,
    dialogs::STORY,
    tmux_styles::STORY,
    terminal::STORY,
    phone::STORY,
];
