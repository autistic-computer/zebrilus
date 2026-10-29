#![allow(clippy::pedantic, clippy::style, clippy::complexity)]
#![allow(dead_code)]

mod auto_scroll;
mod document;
mod global;
mod inline;
mod inline_flow;
mod links;
mod markdown_ext;
mod markdown_parse;
mod node;
#[cfg(test)]
mod perf;
mod scroll_area;
mod selection;
mod state;
mod style;
mod text_view;
mod utils;
mod veil;
mod window_selection;

use zz_gpui::{App, KeyBinding, actions};

pub use links::{LinkClick, TextLinks};
pub use markdown_ext::{
    MarkdownBlockParserFn, MarkdownBlockRenderFn, MarkdownExtensions, MarkdownNode,
    MarkdownParseContext, MarkdownPlugin, markdown_ast,
};
pub use node::{CODE_BLOCK_GROUP, CodeBlock};
pub use state::TextViewState;
pub use style::TextViewStyle;
pub use text_view::{TextView, TextViewLayoutState};

pub use global::suppress_text_selection;

pub(crate) use window_selection::{SelectionScope, TextSelectionController, WindowTextSelection};

const CONTEXT: &str = "TextView";

pub const ROOT_KEY_CONTEXT: &str = "ZzRoot";

actions!(zz_text, [Copy, SelectAll]);

pub(crate) fn clipboard_selection_text(selected_text: &str) -> &str {
    selected_text.strip_suffix('\n').unwrap_or(selected_text)
}

pub(crate) fn init(cx: &mut App) {
    global::init(cx);

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let (copy, select_all) = ("cmd-c", "cmd-a");
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let (copy, select_all) = ("ctrl-c", "ctrl-a");

    cx.bind_keys([
        KeyBinding::new(copy, Copy, Some(CONTEXT)),
        KeyBinding::new(select_all, SelectAll, Some(CONTEXT)),
        KeyBinding::new(copy, Copy, Some(ROOT_KEY_CONTEXT)),
    ]);
}
