mod actions;
mod delegate;
mod element;
mod state;

use ala::{App, KeyBinding};

pub use delegate::{SelectDelegate, SelectItem};
pub use element::Select;
pub use state::{SelectEvent, SelectState};

use actions::{Cancel, Confirm, SelectNext, SelectPrev};

const CONTEXT: &str = "ZzSelect";

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrev, Some(CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
        KeyBinding::new("space", Confirm, Some("ZzSelect && !ZzInput")),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
    ]);
}
