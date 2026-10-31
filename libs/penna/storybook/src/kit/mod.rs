use crate::story::Story;

mod buttons;
mod code_editor;
mod color_picker;
mod compact;
mod dialogs;
mod display;
mod feedback;
mod icons;
mod inputs;
mod markdown;
pub mod menus;
mod navigation;
mod notifications;
mod scrollbars;
mod selection;
mod sheet;
mod support;
mod title_bar;
pub mod which_key;

pub const STORIES: &[Story] = &[
    buttons::STORY,
    inputs::STORY,
    selection::STORY,
    display::STORY,
    menus::STORY,
    dialogs::STORY,
    feedback::STORY,
    notifications::STORY,
    sheet::STORY,
    scrollbars::STORY,
    markdown::STORY,
    code_editor::STORY,
    color_picker::STORY,
    icons::STORY,
    title_bar::STORY,
    navigation::STORY,
    which_key::STORY,
    compact::STORY,
];
