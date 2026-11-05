use crate::story::Story;

mod appearance;
mod buttons;
mod canvas;
mod chat;
mod checks;
#[cfg(feature = "editor")]
mod code_editor;
mod color_picker;
mod compact;
mod dialogs;
mod display;
mod docs;
mod feedback;
mod icons;
mod inputs;
pub(crate) mod layout;
mod markdown;
pub mod menus;
mod navigation;
mod notifications;
mod palette;
mod panels;
mod rows;
mod scrollbars;
mod selection;
mod settings;
mod sheet;
mod status;
mod support;
mod title_bar;
pub mod which_key;

pub const STORIES: &[Story] = &[
    buttons::STORY,
    inputs::STORY,
    selection::STORY,
    checks::STORY,
    display::STORY,
    menus::STORY,
    palette::STORY,
    dialogs::STORY,
    feedback::STORY,
    status::STORY,
    canvas::STORY,
    panels::STORY,
    rows::STORY,
    notifications::STORY,
    sheet::STORY,
    scrollbars::STORY,
    markdown::STORY,
    #[cfg(feature = "editor")]
    code_editor::STORY,
    color_picker::STORY,
    icons::STORY,
    title_bar::STORY,
    navigation::STORY,
    settings::STORY,
    appearance::STORY,
    which_key::STORY,
    compact::STORY,
    chat::STORY,
    docs::STORY,
];
