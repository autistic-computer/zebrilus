use crate::story::Story;

mod colors;
mod design_system;
mod icon_set;

pub const STORIES: &[Story] = &[colors::STORY, design_system::STORY, icon_set::STORY];
