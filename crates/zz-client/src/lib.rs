//! The renderer-free client brain shared by every zz client.
//!
//! [`ClientCore`] is a sans-IO state machine: decoded [`zz_protocol::ProtocolMessage`]s
//! go in, typed [`CoreEvent`]s and [`Outbound`] requests come out, and the
//! reduced state (snapshot, viewports, overlays, key tables) is read through
//! plain accessors. It owns no socket, spawns no thread, and reads no clock,
//! so a shell can drive it from any runtime — a zz-gpui entity, a TUI reader
//! thread, a deterministic simulator, or a C caller behind FFI.

pub mod agent_completion;
pub mod agent_lens;
pub mod agent_output;
pub mod agent_question;
pub mod agent_timeline;
pub mod agent_transcript;
pub mod chooser;
mod chrome;
pub mod completion;
mod core;
#[cfg(not(target_family = "wasm"))]
pub mod element_picker;
pub mod floating;
pub mod fuzzy;
pub mod gui_actions;
mod input;
mod layout;
pub mod local_scroll;
pub mod mend;
mod menu;
pub mod navigation;
pub mod new_chat;
pub mod palette;
pub mod pane_picker;
pub mod pane_separator;
pub mod path_insert;
pub mod path_rank;
pub mod place;
pub mod program;
pub mod providers;
pub mod recents;
pub mod scrollback;
pub mod sidebar;
mod status;
mod status_bar;
pub mod terminal_color;
pub mod terminal_view;
pub mod tmux_style;
pub mod verbs;
pub mod which_key;
pub mod window_summary;

pub use chrome::{
    BROWSER_TABLE, CHROME_TABLES, ChromeAction, ChromeKey, ChromeKeymap, ChromeProfile,
    PREFIX_TABLE, SIDEBAR_TABLE, TERMINAL_TABLE, UI_TABLE, UnknownChromeAction,
};
pub use core::{
    AgentAttentionEdge, AgentAttentionStatus, ClientCore, CoreEvent, Outbound, ViewportDamage,
    agent_attention_status,
};
pub use input::{
    Disposition, Effect, InputEvent, InputOwner, InputRouter, PrefixView, SurfaceKind,
};
pub use layout::{
    DropZone, NormalizedPaneRect, PaneRect, coerced_drop_zone, drop_preview_bounds, drop_zone_at,
    pane_box, pane_drop_command, pane_join_command, pane_rects, pane_swap_command,
    picker_split_command, predicted_drop_layout, same_arrangement, split_ratio_basis,
};
pub use menu::{
    MENU_LINE_HEIGHT, MENU_LIST_PADDING, MENU_SEPARATOR_HEIGHT, MENU_TITLE_HEIGHT,
    MENU_WINDOW_MARGIN, MOUSE_BUTTON_1, MenuBox, MenuKeyResult, MenuPasteResult, MenuPointerKind,
    confirm_accepts, menu_hit_cell, menu_row_height, resolve_menu_key, resolve_menu_mouse,
    resolve_menu_paste,
};
pub use status::{ComposedStatusRow, StatusHitRange, compose_status_row, compose_status_row_over};
pub use status_bar::{
    MAX_VISIBLE_WINDOWS, StatusBarModel, StatusBarPane, StatusBarSettings, StatusBarWindow,
    visible_window_range,
};

pub mod appearance;
pub mod url_input;
