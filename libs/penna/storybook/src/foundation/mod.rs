use crate::story::Story;

mod colors;
mod design_system;
mod icon_set;
mod spacing;
mod type_scale;

pub const STORIES: &[Story] = &[
    colors::STORY,
    type_scale::STORY,
    spacing::STORY,
    design_system::STORY,
    icon_set::STORY,
];
