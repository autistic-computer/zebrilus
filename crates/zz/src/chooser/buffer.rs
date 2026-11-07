use std::time::{SystemTime, UNIX_EPOCH};

use ala::{AnyElement, Entity, MouseButton, prelude::*};
use zz_protocol::{ChooseBufferAction, ChooseBufferItem, ChooseBufferState, InputMessage};
use zz_terminal::KeyInput;

use crate::{
    chooser::{Chooser, ChooserHint, ChooserSearch, ChooserSpec},
    mux::client::MuxClient,
    terminal::view::TERMINAL_FONT,
};
use zz_ui::chooser::{buffer_chooser_row, chooser_subtitle};

const BUFFER_HINTS: &[ChooserHint] = &[
    ChooserHint {
        keys: &["up", "down"],
        label: "navigate",
    },
    ChooserHint {
        keys: &["enter"],
        label: "paste",
    },
    ChooserHint {
        keys: &["d"],
        label: "delete",
    },
    ChooserHint {
        keys: &["/"],
        label: "search",
    },
    ChooserHint {
        keys: &["escape"],
        label: "close",
    },
];

pub(crate) struct BufferChooser;

pub(crate) type ChooseBufferView = Chooser<BufferChooser>;

impl ChooserSpec for BufferChooser {
    type State = ChooseBufferState;
    type Item = ChooseBufferItem;
    type Action = ChooseBufferAction;

    const OVERLAY_ID: &'static str = "choose-buffer-overlay";
    const MODAL_ID: &'static str = "choose-buffer-modal";
    const ROWS_ID: &'static str = "choose-buffer-rows";
    const ROW_ID: &'static str = "choose-buffer-row";
    const CLOSE_ID: &'static str = "choose-buffer-close";
    const MAX_WIDTH: f32 = 640.0;
    const HINTS: &'static [ChooserHint] = BUFFER_HINTS;

    fn state(mux: &MuxClient) -> Option<Self::State> {
        mux.choose_buffer().cloned()
    }

    fn selected(state: &Self::State) -> u32 {
        state.selected
    }

    fn items(state: &Self::State) -> &[Self::Item] {
        &state.items
    }

    fn item_key(item: &Self::Item) -> &str {
        &item.key
    }

    fn search(state: &Self::State) -> Option<ChooserSearch<'_>> {
        state.search.as_ref().map(|search| ChooserSearch {
            query: &search.query,
            reverse: search.reverse,
        })
    }

    fn help(state: &Self::State) -> bool {
        state.help
    }

    fn prompt(state: &Self::State) -> Option<&str> {
        (!state.prompt.is_empty()).then_some(state.prompt.as_str())
    }

    fn title(_: &Self::State) -> &'static str {
        "Paste buffer"
    }

    fn subtitle(state: &Self::State, count: usize) -> String {
        chooser_subtitle(format!("{count} buffers"), state.filter_no_matches)
    }

    fn row(
        item: Self::Item,
        index: usize,
        selected: bool,
        show_key_gutter: bool,
        mux: Entity<MuxClient>,
    ) -> AnyElement {
        buffer_row(item, index, selected, show_key_gutter, mux)
    }

    fn key(input: KeyInput) -> Self::Action {
        ChooseBufferAction::Key(input)
    }

    fn search_append(text: String) -> Self::Action {
        ChooseBufferAction::SearchAppend(text)
    }

    fn close() -> Self::Action {
        ChooseBufferAction::Close
    }

    fn send(mux: &MuxClient, action: Self::Action) {
        mux.send_input(InputMessage::ChooseBuffer { action });
    }
}

fn buffer_row(
    item: ChooseBufferItem,
    index: usize,
    selected: bool,
    show_key_gutter: bool,
    mux: Entity<MuxClient>,
) -> AnyElement {
    let (name, preview, size, age) = buffer_row_text(&item);
    buffer_chooser_row(
        BufferChooser::ROW_ID,
        index,
        item.key,
        show_key_gutter,
        name,
        preview,
        size,
        age,
        item.tagged,
        selected,
        TERMINAL_FONT,
    )
    .on_mouse_down(MouseButton::Left, move |event, _, cx| {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        BufferChooser::send(
            mux.read(cx),
            if event.click_count >= 2 {
                ChooseBufferAction::PasteIndex(index)
            } else {
                ChooseBufferAction::Select(index)
            },
        );
        cx.stop_propagation();
    })
    .into_any_element()
}

fn buffer_row_text(item: &ChooseBufferItem) -> (String, String, String, String) {
    let now = if item.text.is_empty() {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    } else {
        0
    };
    zz_client::chooser::buffer_row_text(item, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_ages_are_compact() {
        let mut item = ChooseBufferItem {
            name: String::new(),
            preview: String::new(),
            size_bytes: 999,
            created_unix_seconds: u64::MAX,
            key: String::new(),
            text: String::new(),
            tagged: false,
        };
        let (_, _, size, age) = buffer_row_text(&item);
        assert_eq!(size, "999 B");
        assert_eq!(age, "0s");
        item.size_bytes = 1_536;
        assert_eq!(buffer_row_text(&item).2, "1.5 KiB");
    }

    #[test]
    fn a_row_format_replaces_the_whole_row_the_way_mode_tree_add_does() {
        let mut item = ChooseBufferItem {
            name: "buffer0".to_owned(),
            preview: "hello".to_owned(),
            size_bytes: 5,
            created_unix_seconds: 0,
            key: "0".to_owned(),
            text: String::new(),
            tagged: false,
        };
        let (name, preview, size, _) = buffer_row_text(&item);
        assert_eq!(
            (name, preview, size),
            ("buffer0".to_owned(), "hello".to_owned(), "5 B".to_owned())
        );
        item.text = "<<buffer0>>".to_owned();
        assert_eq!(
            buffer_row_text(&item),
            (
                "<<buffer0>>".to_owned(),
                String::new(),
                String::new(),
                String::new()
            )
        );
    }
}
