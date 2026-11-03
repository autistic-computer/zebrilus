#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use crate::blocks::dialog;

use ala::{App, KeyBinding};

pub(crate) mod actions;
mod root;
mod toast;
pub(crate) mod window_ext;

pub use crate::components::text::ROOT_KEY_CONTEXT;
pub use root::Root;
pub use toast::{push_toast, set_toast_host};
pub use window_ext::WindowExt;

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", actions::Tab, Some(root::CONTEXT)),
        KeyBinding::new("shift-tab", actions::TabPrev, Some(root::CONTEXT)),
        KeyBinding::new("escape", actions::CancelDialog, Some(dialog::CONTEXT)),
        KeyBinding::new("enter", actions::ConfirmDialog, Some(dialog::CONTEXT)),
    ]);
}
