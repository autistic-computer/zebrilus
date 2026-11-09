#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

mod button;
mod button_icon;

pub(crate) use button::rings_focus;
pub use button::{
    Button, ButtonLook, ButtonRounded, ButtonVariant, ButtonVariants, COMPACT_ICON_BUTTON_SIZE,
    INSIDE_GAP,
};
