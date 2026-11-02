#![allow(
    clippy::cast_precision_loss,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::needless_pass_by_value,
    clippy::struct_excessive_bools
)]

mod actions;
mod element;
mod field;
mod history;
mod number;
mod state;

pub use actions::{
    Backspace, Copy, Cut, Delete, DeleteToBeginningOfLine, DeleteToEndOfLine, DeleteToNextWordEnd,
    DeleteToPreviousWordStart, Enter, Escape, IndentInline, MoveDown, MoveEnd, MoveHome, MoveLeft,
    MoveRight, MoveToEnd, MoveToNextWord, MoveToPreviousWord, MoveToStart, MoveUp, OutdentInline,
    Paste, Redo, SelectAll, SelectDown, SelectLeft, SelectRight, SelectToEnd, SelectToEndOfLine,
    SelectToNextWordEnd, SelectToPreviousWordStart, SelectToStart, SelectToStartOfLine, SelectUp,
    ShowCharacterPalette, Undo,
};
pub use ala::TextAlign;
pub use field::{FieldLook, Input, InputContentType, field_chrome, field_paint, field_underlay};
pub(crate) use field::{field_chrome_on, field_paint_on, field_text};
pub use number::NumberInput;
pub use state::{InputEvent, InputState};

pub(crate) fn init(cx: &mut ala::App) {
    actions::init(cx);
    number::init(cx);
}
