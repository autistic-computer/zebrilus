#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use ala::{App, KeyBinding};

mod actions;
mod dialog;
mod notification;
mod root;
mod toast;
mod window_ext;

pub use crate::text::ROOT_KEY_CONTEXT;
pub use dialog::{AlertDialog, Dialog, DialogButtonProps};
pub use dialog::{dialog_description, dialog_footer, dialog_gutter, dialog_surface, dialog_title};
pub use notification::Notification;
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
