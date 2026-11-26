mod callbacks;
mod customize;
mod hooks;
mod key_bindings;
mod mode_prompt;
mod option_formats;
mod option_lookup;
mod option_store;
mod pane_spawn;
mod panes;
mod panes_mode;
mod sessions;
mod set_option;
mod show_options;
mod switch_mode;
mod windows;
pub(crate) use callbacks::format_key_command;
use callbacks::*;
pub use callbacks::{format_command, if_shell_truthy, validate_static_command_chain};
pub use customize::{
    CUSTOMIZE_MENU_ITEMS, CUSTOMIZE_OUTSIDE_MENU_ITEMS, CustomizeEdit, CustomizeMenu,
    CustomizeMenuItem, CustomizeMode, CustomizeResult, customize_menu_feed,
};
pub use hooks::hook_format_variables;
use hooks::*;
use key_bindings::*;
pub use key_bindings::{mouse_key_is_a_replayed_double_click, parse_tmux_key};
pub use mode_prompt::{ModeKey, ModePrompt, PromptHistories, PromptOutcome};
use option_formats::*;
use option_lookup::*;
use option_store::*;
pub use pane_spawn::check_plain_text;
use pane_spawn::*;
pub use panes_mode::{PanesModeAreaGeometry, PanesModeGeometry};
use set_option::*;
use show_options::*;
pub use switch_mode::{SwitchAction, SwitchMode};
use windows::*;

use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt::{self, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Weak},
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use zz_protocol::{
    AGENT_FLOAT_HEIGHT, AGENT_FLOAT_WIDTH, AgentAutoApprove, AgentDescriptor, AgentProvider, Axis,
    BrowserDescriptor, ChooseTreeKind, ChooserPreviewSize, ClientEnvironmentBlob, ClientId,
    CommandInvocation, CommandPromptMode, CommandPromptType, CommandResolution, CommandSpec,
    DEFAULT_AGENT_AUTO_APPROVE, DEFAULT_AGENT_CLAUDE_CODE_COMMAND, DEFAULT_AGENT_COMMAND,
    DEFAULT_BROWSER_PROFILE, EditorDescriptor, KeyToken, MAX_AGENT_COMMAND_BYTES,
    MAX_AGENT_OPTION_BYTES, MAX_AGENT_SEND_BYTES, MAX_AGENT_SESSION_ID_BYTES, MAX_GUI_TEXT_BYTES,
    MuxOptionKey, NATIVE_COMMAND_NAMES, PaneBorderIndicators, PaneBorderLines, PaneBorderStatus,
    PaneId, PaneKindSnapshot, PopupBorderLines, RawText, SIMULATOR_PANE_KIND, ServerError,
    SessionId, SimulatorDescriptor, SourceSpan, TerminalUiCommand, ViewContent, ViewDescriptor,
    WindowId, catalog_command_spec, command_specs, normalize_browser_profile_name,
    parse_tmux_command_options, parse_tmux_options, resolve_command,
    unimplemented_tmux_command_spec,
};
use zz_terminal::{
    CopyJump, CopyJumpDirection, CopyModeAction, CopyModeCopy, CopyModeCountPolicy, CopyModeSearch,
    CopySelectionMode, DEFAULT_HISTORY_LIMIT, DEFAULT_WORD_SEPARATORS, MAX_HISTORY_LIMIT,
    PasteBufferAction, SearchDirection, TerminalViewAction,
};

use crate::{
    BellAction, Binding, KeyTables, LayoutPreset, MuxState, PaneDirection, PaneKind, PresetOptions,
    SplitPlacement, SplitSize as LayoutSplitSize, StatusContext, StatusFormats, StatusOption,
    TmuxSort, TmuxSortOrder, VisualBell, WindowSize, WindowStatusFormats, WindowStatusOption,
    canonical_command, command_spec,
    copy_actions::pinned_copy_action,
    formats::{
        CommandHooks, FormatClient, FormatClientRow, FormatContext, FormatEnvironRow, FormatJobTag,
        FormatOptionRow, FormatType, LayoutDumps, PreparedFormat, StatusHooks,
        expand_format_time_traced, expand_format_time_with_hooks, expand_format_with_hooks,
        format_listing, format_true, parse_tmux_colour,
    },
    honest_knobs::{
        AllowPassthrough, PaneOption, PaneOptions, ServerOption, ServerOptions, SessionOption,
        SessionOptions, WindowOption, WindowOptions,
    },
    layout::{CellGeometry, CellLayout, PANE_MAXIMUM, PANE_MINIMUM, split_floating_cell},
    model::{
        DEFAULT_WINDOW_EXTENT, FloatSpawn, FloatZ, GlobPattern, Modal, NO_MARKED_TARGET,
        is_marked_target, simulator_descriptor,
    },
    terminfo::TtyTerm,
    tmux_options::{
        HOOK_NAMES, TmuxArrayValue, TmuxOption, TmuxOptionScope, TmuxStoredScalarKind,
        exact_tmux_option, match_tmux_option, parse_tmux_option, tmux_consumer_options,
        tmux_option_format_is_flag, tmux_option_is_hook, tmux_option_table_order,
        tmux_option_with_consumer, tmux_options, tmux_stored_array, tmux_stored_scalar,
    },
    valid_style,
};

#[cfg(test)]
use crate::tmux_options::{
    MESSAGE_COMMAND_STYLE_DEFAULT, MESSAGE_FORMAT_DEFAULT, MESSAGE_STYLE_DEFAULT,
    PANE_SCROLLBARS_STYLE_DEFAULT,
};
#[cfg(test)]
use zz_protocol::{COMMAND_SPECS, DAEMON_COMMAND_SPECS};

pub const TMUX_OPTION_CONSUMERS: &[&str] = &[
    "base-index",
    "pane-base-index",
    "renumber-windows",
    "default-size",
    "window-size",
    "aggressive-resize",
    "history-limit",
    "detach-on-destroy",
    "prefix",
    "mode-keys",
    "status-keys",
    "key-table",
    "prefix-timeout",
    "repeat-time",
    "initial-repeat-time",
    "prompt-history-limit",
    "history-file",
    "word-separators",
    "wrap-search",
    "default-shell",
    "default-command",
    "default-terminal",
    "editor",
    "remain-on-exit",
    "focus-events",
    "extended-keys",
    "terminal-features",
    "user-keys",
    "allow-passthrough",
    "allow-rename",
    "allow-set-title",
    "alternate-screen",
    "remain-on-exit-format",
    "scroll-on-clear",
    "backspace",
    "cursor-style",
    "cursor-colour",
    "synchronize-panes",
    "automatic-rename",
    "automatic-rename-format",
    "bell-action",
    "visual-bell",
    "display-time",
    "display-panes-time",
    "display-panes-colour",
    "display-panes-active-colour",
    "display-panes-border-style",
    "message-limit",
    "buffer-limit",
    "set-clipboard",
    "copy-command",
    "menu-border-lines",
    "menu-border-style",
    "menu-selected-style",
    "menu-style",
    "popup-border-lines",
    "popup-border-style",
    "popup-style",
    "main-pane-width",
    "main-pane-height",
    "other-pane-width",
    "other-pane-height",
    "tiled-layout-max-columns",
    "status",
    "status-interval",
    "status-left",
    "status-right",
    "status-left-length",
    "status-right-length",
    "status-left-style",
    "status-right-style",
    "status-style",
    "status-bg",
    "status-fg",
    "status-format",
    "status-justify",
    "status-position",
    "message-line",
    "message-style",
    "pane-status-style",
    "pane-status-current-style",
    "session-status-style",
    "session-status-current-style",
    "window-pane-status-format",
    "window-pane-current-status-format",
    "window-status-format",
    "window-status-current-format",
    "window-status-separator",
    "window-status-style",
    "window-status-current-style",
    "window-status-last-style",
    "window-status-bell-style",
    "mouse",
    "focus-follows-mouse",
    "default-client-command",
    "escape-time",
    "set-titles",
    "set-titles-string",
    "command-alias",
    "update-environment",
    "exit-empty",
    "exit-unattached",
    "destroy-unattached",
    "monitor-activity",
    "monitor-bell",
    "monitor-silence",
    "activity-action",
    "silence-action",
    "visual-activity",
    "visual-silence",
    "window-status-activity-style",
    "prefix2",
    "display-panes-format",
    "window-style",
    "window-active-style",
    "mode-style",
    "clock-mode-colour",
    "clock-mode-style",
    "pane-border-style",
    "pane-active-border-style",
    "pane-border-format",
    "pane-border-indicators",
    "pane-border-lines",
    "pane-border-status",
    "pane-colours",
    "copy-mode-match-style",
    "copy-mode-current-match-style",
    "copy-mode-mark-style",
    "copy-mode-position-format",
    "copy-mode-position-style",
    "copy-mode-selection-style",
    "copy-mode-line-numbers",
    "copy-mode-line-number-style",
    "copy-mode-current-line-number-style",
    "copy-mode-current-line-style",
    "clear-on-attach",
    "switch-mode-match-style",
    "tree-mode-border-style",
    "tree-mode-preview-format",
    "tree-mode-preview-style",
    "tree-mode-selection-style",
    "message-command-style",
    "prompt-cursor-style",
    "prompt-cursor-colour",
    "prompt-command-cursor-style",
    "prompt-command-cursor-colour",
    "theme",
    "dark-theme-black",
    "dark-theme-white",
    "dark-theme-light-grey",
    "dark-theme-dark-grey",
    "dark-theme-green",
    "dark-theme-yellow",
    "dark-theme-red",
    "dark-theme-blue",
    "dark-theme-cyan",
    "dark-theme-magenta",
    "light-theme-black",
    "light-theme-white",
    "light-theme-light-grey",
    "light-theme-dark-grey",
    "light-theme-green",
    "light-theme-yellow",
    "light-theme-red",
    "light-theme-blue",
    "light-theme-cyan",
    "light-theme-magenta",
];

const MAX_COPY_COMMAND_BYTES: usize = 8 * 1024;
const SIMULATOR_FLOAT_WIDTH: u16 = 35;
const SIMULATOR_FLOAT_HEIGHT: u16 = 36;
const SIMULATOR_FLOAT_PLACE: &str = "centre-right";
const MAX_COMMAND_PROMPT_LABEL_BYTES: usize = 1024;
const MAX_COMMAND_PROMPT_TEMPLATE_BYTES: usize = 8 * 1024;
const DEFAULT_DISPLAY_MESSAGE: &str = "[#{session_name}] #{window_index}:#{window_name}, current pane #{pane_index} - (%H:%M %d-%b-%y)";
const DEFAULT_NEW_SESSION_FORMAT: &str = "#{session_name}:";
const DEFAULT_PANE_CREATION_FORMAT: &str = "#{session_name}:#{window_index}.#{pane_index}";
const COMMAND_ITEM_CONTEXT_FORMATS: &[&str] = &["command"];
const ROW_CONTEXT_FORMATS: &[&str] = &["line"];
const COPY_MODE_CONTEXT_FORMATS: &[&str] = &[
    "copy_cursor_line",
    "copy_cursor_word",
    "copy_cursor_x",
    "copy_cursor_y",
    "copy_line_numbers",
    "rectangle_toggle",
    "refresh_active",
    "scroll_position",
    "search_count",
    "search_count_partial",
    "search_match",
    "search_present",
    "search_timed_out",
    "selection_active",
    "selection_end_x",
    "selection_end_y",
    "selection_present",
    "selection_start_x",
    "selection_start_y",
];
const DEFAULT_LIST_COMMANDS_FORMAT: &str =
    "#{command_list_name}#{?command_list_alias, (#{command_list_alias}),} #{command_list_usage}";
const LIST_COMMAND_CONTEXT_FORMATS: &[&str] = &[
    "command_list_alias",
    "command_list_name",
    "command_list_usage",
];
const DEFAULT_LIST_KEYS_FORMAT: &str = concat!(
    "#{?notes_only,",
    "#{key_prefix} ",
    "#{p|#{key_string_width}:key_string} ",
    "#{?key_note,#{key_note},#{key_command}}",
    ",",
    "bind-key #{?key_has_repeat,#{?key_repeat,-r,  },} ",
    "-T #{p|#{key_table_width}:key_table} ",
    "#{p|#{key_string_width}:#{q|a:key_string}} ",
    "#{key_command}}",
);
const LIST_KEY_BINDING_CONTEXT_FORMATS: &[&str] = &[
    "key_command",
    "key_note",
    "key_prefix",
    "key_repeat",
    "key_string",
    "key_table",
];
const LIST_KEY_SUMMARY_CONTEXT_FORMATS: &[&str] = &[
    "key_has_repeat",
    "key_string_width",
    "key_table_width",
    "notes_only",
];
static LIST_KEY_FORMAT_NAMES_ARE_NOT_OPTIONS: LazyLock<bool> = LazyLock::new(|| {
    LIST_KEY_BINDING_CONTEXT_FORMATS
        .iter()
        .chain(LIST_KEY_SUMMARY_CONTEXT_FORMATS)
        .all(|name| !name.starts_with('@') && parse_format_option(name).is_none())
});
const CURRENT_FILE_CONTEXT_FORMAT: &str = "current_file";
const HOOK_CONTEXT_FORMAT: &str = "hook";
const HOOK_ARGUMENTS_CONTEXT_FORMAT: &str = "hook_arguments";
const HOOK_ARGUMENT_CONTEXT_PATTERN: &str = "hook_argument_N";
const HOOK_ARGUMENT_CONTEXT_PREFIX: &str = "hook_argument_";
const HOOK_LAST_CONTEXT_FORMAT: &str = "hook_last";
const HOOK_PANE_CONTEXT_FORMAT: &str = "hook_pane";
const HOOK_SESSION_CONTEXT_FORMAT: &str = "hook_session";
const HOOK_SESSION_NAME_CONTEXT_FORMAT: &str = "hook_session_name";
const HOOK_VALUE_CONTEXT_FORMAT: &str = "hook_value";
const HOOK_WINDOW_CONTEXT_FORMAT: &str = "hook_window";
const HOOK_WINDOW_INDEX_CONTEXT_FORMAT: &str = "hook_window_index";
const HOOK_WINDOW_NAME_CONTEXT_FORMAT: &str = "hook_window_name";
const HOOK_FLAG_CONTEXT_PATTERN: &str = "hook_flag_X";
const HOOK_FLAG_CONTEXT_PREFIX: &str = "hook_flag_";
const HOOK_FLAG_VALUE_CONTEXT_PATTERN: &str = "hook_flag_X_N";

const LITERAL_FORMAT_CONTEXT_SCOPES: &[(&str, &str, &[&str])] = &[
    (
        "cmd-list-commands.c",
        "cmd_list_single_command",
        LIST_COMMAND_CONTEXT_FORMATS,
    ),
    (
        "cmd-list-keys.c",
        "cmd_list_keys_exec",
        LIST_KEY_SUMMARY_CONTEXT_FORMATS,
    ),
    (
        "cmd-list-keys.c",
        "cmd_list_keys_format_add_key_binding",
        LIST_KEY_BINDING_CONTEXT_FORMATS,
    ),
    (
        "cmd-list-panes.c",
        "cmd_list_panes_window",
        ROW_CONTEXT_FORMATS,
    ),
    (
        "cmd-list-sessions.c",
        "cmd_list_sessions_exec",
        ROW_CONTEXT_FORMATS,
    ),
    (
        "cmd-list-windows.c",
        "cmd_list_windows_exec",
        ROW_CONTEXT_FORMATS,
    ),
    (
        "cmd-queue.c",
        "cmdq_merge_formats",
        COMMAND_ITEM_CONTEXT_FORMATS,
    ),
    (
        "cmd-show-options.c",
        "cmd_show_hooks_print_monitor",
        SHOW_HOOKS_MONITOR_CONTEXT_FORMATS,
    ),
    (
        "cmd-show-options.c",
        "cmd_show_options_print",
        SHOW_OPTIONS_CONTEXT_FORMATS,
    ),
    (
        "cmd-show-options.c",
        "cmd_show_hooks_print_monitor",
        SHOW_HOOK_FIRE_CONTEXT_FORMATS,
    ),
    (
        "cmd-show-options.c",
        "cmd_show_options_print",
        SHOW_HOOK_FIRE_CONTEXT_FORMATS,
    ),
    ("hooks.c", "hooks_insert_event", &[HOOK_CONTEXT_FORMAT]),
    ("hooks.c", "hooks_run", &[HOOK_CONTEXT_FORMAT]),
    (
        "window-client.c",
        "window_client_draw_info",
        &["clipboard_invalid"],
    ),
    (
        "window-copy.c",
        "window_copy_formats",
        COPY_MODE_CONTEXT_FORMATS,
    ),
];

const DERIVED_FORMAT_CONTEXT_FAMILIES: &[(&str, &[&str], &[&str])] =
    &[("current-file", &[CURRENT_FILE_CONTEXT_FORMAT], &[])];

const HOOK_PAYLOAD_CONTEXT_FORMATS: &[&str] = &[
    HOOK_ARGUMENTS_CONTEXT_FORMAT,
    "hook_client",
    "hook_command_duration",
    "hook_command_end_time",
    "hook_command_start_time",
    "hook_command_status",
    "hook_created_empty",
    "hook_created_respawn",
    "hook_current_mode",
    "hook_event",
    "hook_exit_signal",
    "hook_exit_status",
    "hook_exit_success",
    "hook_height",
    HOOK_LAST_CONTEXT_FORMAT,
    "hook_marked",
    "hook_mode_entered",
    "hook_new_name",
    "hook_new_pane",
    "hook_new_session",
    "hook_new_session_name",
    "hook_new_title",
    "hook_new_window",
    "hook_new_window_index",
    "hook_new_window_name",
    "hook_old_height",
    "hook_old_name",
    "hook_old_pane",
    "hook_old_session",
    "hook_old_session_name",
    "hook_old_width",
    "hook_old_window",
    "hook_old_window_index",
    "hook_old_window_name",
    HOOK_PANE_CONTEXT_FORMAT,
    "hook_pane_command",
    "hook_pane_current_path",
    "hook_paste_buffer",
    "hook_previous_mode",
    "hook_prompt_type",
    HOOK_SESSION_CONTEXT_FORMAT,
    HOOK_SESSION_NAME_CONTEXT_FORMAT,
    HOOK_VALUE_CONTEXT_FORMAT,
    "hook_width",
    HOOK_WINDOW_CONTEXT_FORMAT,
    HOOK_WINDOW_INDEX_CONTEXT_FORMAT,
    HOOK_WINDOW_NAME_CONTEXT_FORMAT,
];
const HOOK_PAYLOAD_CONTEXT_PATTERNS: &[&str] = &[
    HOOK_ARGUMENT_CONTEXT_PATTERN,
    HOOK_FLAG_CONTEXT_PATTERN,
    HOOK_FLAG_VALUE_CONTEXT_PATTERN,
];

const ACCEPTED_NATIVE_LITERAL_FORMAT_CONTEXT_SCOPES: &[(&str, &str, &[&str])] = &[
    (
        "mode-tree.c",
        "mode_tree_draw",
        &[
            "mode_tree_branch",
            "mode_tree_expanded",
            "mode_tree_flat",
            "mode_tree_has_children",
            "mode_tree_key",
            "mode_tree_key_width",
            "mode_tree_last",
            "mode_tree_parent_last",
            "mode_tree_repeat",
            "mode_tree_selected",
        ],
    ),
    ("prompt.c", "prompt_expand1", &["message"]),
    (
        "prompt.c",
        "prompt_format_tree",
        &[
            "command_prompt",
            "prompt_flags",
            "prompt_input",
            "prompt_type",
        ],
    ),
    (
        "status.c",
        "status_message_redraw",
        &["command_prompt", "message"],
    ),
    (
        "window-border.c",
        "window_set_fill_cell",
        &["is_inside", "is_outside"],
    ),
    (
        "window-client.c",
        "window_client_get_key",
        ROW_CONTEXT_FORMATS,
    ),
    (
        "window-copy.c",
        "window_copy_formats",
        &[
            "copy_cursor_hyperlink",
            "copy_position",
            "copy_position_limit",
            "selection_mode",
            "top_line_time",
        ],
    ),
    (
        "window-customize.c",
        "window_customize_build",
        &["is_environment", "is_key", "is_option"],
    ),
    (
        "window-customize.c",
        "window_customize_build_array",
        &["option_name", "option_value"],
    ),
    (
        "window-customize.c",
        "window_customize_build_environment",
        &[
            "environment_hidden",
            "environment_is_global",
            "environment_name",
            "environment_removed",
            "environment_scope",
            "environment_value",
            "is_environment",
            "is_key",
            "is_option",
        ],
    ),
    (
        "window-customize.c",
        "window_customize_build_keys",
        &["is_environment", "is_key", "is_option", "key", "key_note"],
    ),
    (
        "window-customize.c",
        "window_customize_build_option",
        &[
            "option_is_array",
            "option_is_global",
            "option_is_hook",
            "option_is_monitor",
            "option_monitor",
            "option_name",
            "option_scope",
            "option_unit",
            "option_value",
        ],
    ),
];
const SHOW_HOOK_FIRE_CONTEXT_FORMATS: &[&str] = &["hook_fire_count", "hook_fire_time"];
const SHOW_OPTIONS_CONTEXT_FORMATS: &[&str] = &[
    "option_array_key",
    "option_has_array_key",
    "option_has_value",
    "option_is_array",
    "option_is_hook",
    "option_is_parent",
    "option_is_string",
    "option_is_user",
    "option_name",
    "option_value",
    "option_value_only",
];
const SHOW_HOOKS_MONITOR_CONTEXT_FORMATS: &[&str] = &[
    "hook_monitor_format",
    "hook_monitor_target",
    "option_array_key",
    "option_has_array_key",
    "option_has_value",
    "option_is_array",
    "option_is_hook",
    "option_is_parent",
    "option_is_string",
    "option_is_user",
    "option_name",
    "option_value",
    "option_value_only",
];
const MISSING_LITERAL_FORMAT_CONTEXT_SCOPES: &[(&str, &str, &[&str])] = &[];
const MISSING_DERIVED_FORMAT_CONTEXT_FAMILIES: &[(&str, &[&str], &[&str])] = &[];

#[doc(hidden)]
pub fn mux_literal_format_context_scopes()
-> impl Iterator<Item = (&'static str, &'static str, &'static [&'static str])> {
    LITERAL_FORMAT_CONTEXT_SCOPES
        .iter()
        .copied()
        .chain(crate::formats::literal_format_context_scopes())
}

#[doc(hidden)]
pub fn accepted_native_literal_format_context_scopes()
-> impl Iterator<Item = (&'static str, &'static str, &'static [&'static str])> {
    ACCEPTED_NATIVE_LITERAL_FORMAT_CONTEXT_SCOPES
        .iter()
        .copied()
}

#[doc(hidden)]
pub fn missing_literal_format_context_scopes()
-> impl Iterator<Item = (&'static str, &'static str, &'static [&'static str])> {
    MISSING_LITERAL_FORMAT_CONTEXT_SCOPES.iter().copied()
}

#[doc(hidden)]
pub fn mux_derived_format_context_families() -> impl Iterator<
    Item = (
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
    ),
> {
    DERIVED_FORMAT_CONTEXT_FAMILIES
        .iter()
        .copied()
        .chain(crate::formats::derived_format_context_families())
}

#[doc(hidden)]
pub fn mux_hook_payload_format_contexts() -> (&'static [&'static str], &'static [&'static str]) {
    (HOOK_PAYLOAD_CONTEXT_FORMATS, HOOK_PAYLOAD_CONTEXT_PATTERNS)
}

#[doc(hidden)]
pub fn missing_derived_format_context_families() -> impl Iterator<
    Item = (
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
    ),
> {
    MISSING_DERIVED_FORMAT_CONTEXT_FAMILIES.iter().copied()
}

const DEFAULT_LIST_SESSIONS_FORMAT: &str = concat!(
    "#{session_name}: #{session_windows} windows (created #{t:session_created})",
    "#{?session_grouped, (group ,}#{session_group}#{?session_grouped,),}",
    "#{?session_attached, (attached),}",
);
const DEFAULT_LIST_WINDOWS_FORMAT: &str = concat!(
    "#{window_index}: #{window_name}#{window_raw_flags} (#{window_panes} panes) ",
    "[#{window_width}x#{window_height}] [layout #{window_layout}] #{window_id}",
    "#{?window_active, (active),}",
);
const DEFAULT_LIST_WINDOWS_WITH_SESSION_FORMAT: &str = concat!(
    "#{session_name}:#{window_index}: #{window_name}#{window_raw_flags} ",
    "(#{window_panes} panes) [#{window_width}x#{window_height}] ",
);
const DEFAULT_LIST_PANES_FORMAT: &str = concat!(
    "#{pane_index}: [#{pane_width}x#{pane_height}",
    "#{?pane_floating_flag, #{pane_x}#,#{pane_y}#,#{pane_z}}] ",
    "[history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}",
    "#{?pane_active, (active),}#{?pane_dead, (dead),}",
);
const DEFAULT_LIST_PANES_WITH_SESSION_FORMAT: &str = concat!(
    "#{window_index}.#{pane_index}: [#{pane_width}x#{pane_height}",
    "#{?pane_floating_flag, #{pane_x}#,#{pane_y}#,#{pane_z}}] ",
    "[history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}",
    "#{?pane_active, (active),}#{?pane_dead, (dead),}",
);
const DEFAULT_LIST_PANES_WITH_SERVER_FORMAT: &str = concat!(
    "#{session_name}:#{window_index}.#{pane_index}: [#{pane_width}x#{pane_height}",
    "#{?pane_floating_flag, #{pane_x}#,#{pane_y}#,#{pane_z}}] ",
    "[history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}",
    "#{?pane_active, (active),}#{?pane_dead, (dead),}",
);
pub const DEFAULT_BUFFER_LIMIT: usize = 50;
const MAX_BUFFER_LIMIT: usize = i32::MAX.cast_unsigned() as usize;
const DEFAULT_MESSAGE_LIMIT: usize = 1_000;
const MAX_MESSAGE_LIMIT: usize = i32::MAX.cast_unsigned() as usize;
const DEFAULT_HISTORY_TRICKLE: usize = 2_000;
const MAX_HISTORY_TRICKLE: usize = 10_000;
const DEFAULT_BASE_INDEX: u32 = 0;
const MAX_BASE_INDEX: u32 = i32::MAX.cast_unsigned();
const DEFAULT_PANE_BASE_INDEX: u32 = 0;
const MAX_PANE_BASE_INDEX: u32 = u16::MAX as u32;
const DEFAULT_RENUMBER_WINDOWS: bool = false;
const DEFAULT_PREFIX: &str = "C-b";
const DEFAULT_MOUSE: bool = true;
const DEFAULT_ESCAPE_TIME_MS: u32 = 10;
const DEFAULT_AUTOMATIC_RENAME_FORMAT: &str =
    "#{?pane_in_mode,[tmux],#{pane_current_command}}#{?pane_dead,[dead],}";
const DEFAULT_SHELL: &str = "/bin/sh";
const DEFAULT_TERMINAL: &str = "tmux-256color";
const DEFAULT_DISPLAY_TIME_MS: u32 = 750;
const DEFAULT_INITIAL_REPEAT_TIME_MS: u32 = 0;
const DEFAULT_REPEAT_TIME_MS: u32 = 500;
const MAX_REPEAT_TIME_MS: u32 = 2_000_000;
pub const MAX_WORD_SEPARATORS_BYTES: usize = 8 * 1024;

struct RowFormatHooks<'a, H> {
    inner: &'a mut H,
    line: usize,
}

impl<H: StatusHooks> StatusHooks for RowFormatHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        if ROW_CONTEXT_FORMATS.contains(&name) {
            Some(Cow::Owned(self.line.to_string()))
        } else {
            self.inner.tree_variable(name, context)
        }
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        if ROW_CONTEXT_FORMATS.contains(&name) {
            None
        } else {
            self.inner.variable(name, context)
        }
    }

    fn window_activity(&mut self, window: WindowId) -> u64 {
        self.inner.window_activity(window)
    }

    fn pane_activity(&mut self, pane: PaneId) -> u64 {
        self.inner.pane_activity(pane)
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }
}

struct ShownOptionHooks<'a, H> {
    inner: &'a mut H,
    values: &'a [(&'static str, String)],
}

impl<H: StatusHooks> StatusHooks for ShownOptionHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        match self.values.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => Some(Cow::Borrowed(value.as_str())),
            None => self.inner.tree_variable(name, context),
        }
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        if self.values.iter().any(|(key, _)| *key == name) {
            None
        } else {
            self.inner.variable(name, context)
        }
    }

    fn window_activity(&mut self, window: WindowId) -> u64 {
        self.inner.window_activity(window)
    }

    fn pane_activity(&mut self, pane: PaneId) -> u64 {
        self.inner.pane_activity(pane)
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ExecutionContext {
    pub session: Option<SessionId>,
    pub window: Option<WindowId>,
    pub pane: Option<PaneId>,
    client_terminal: ClientTerminal,
    client_size: Option<(u16, u16)>,
    client_working_directory: Option<PathBuf>,
    client_environment: Option<Arc<ClientEnvironmentBlob>>,
    client_attached: bool,
    client_attached_context: Option<(SessionId, WindowId, PaneId)>,
    target_format_client_override: Option<FormatClient>,
    repeat_binding: bool,
    invoking_key: Option<String>,
    invoking_mouse: Option<MouseEventTarget>,
    replay_client: Option<ClientId>,
    control_command_target: Option<(ClientId, u8)>,
    legacy_layouts: bool,
    refuse_new_session_attach: bool,
    spawns_popup: bool,
    pub no_hooks: bool,
    pub format_variables: BTreeMap<String, String>,
    spawn_pane_options: Vec<(String, String)>,
    agent_parent: Option<PaneId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MouseEventTarget {
    pub pane: Option<PaneId>,
    pub window: Option<WindowId>,
    pub column: u16,
    pub row: u16,
    pub border: Option<Axis>,
    pub view_action: Option<TerminalViewAction>,
    pub press_action: Option<TerminalViewAction>,
    pub status_range_start: Option<u16>,
    pub press: Option<(u16, u16)>,
    pub drag: Option<MouseDrag>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseDragKind {
    NewPane,
    MovePane,
    ResizeFloat,
    ResizeTiled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseDrag {
    pub kind: MouseDragKind,
    pub pane: PaneId,
    pub anchor: (u16, u16),
    pub last: (u16, u16),
}

fn mouse_anchor(mouse: &MouseEventTarget) -> (u16, u16) {
    mouse
        .drag
        .map(|drag| drag.anchor)
        .or(mouse.press)
        .unwrap_or((mouse.column, mouse.row))
}

fn mouse_anchor_cell(mouse: &MouseEventTarget) -> (i32, i32) {
    let (x, y) = mouse_anchor(mouse);
    (i32::from(x), i32::from(y))
}

fn mouse_last_cell(mouse: &MouseEventTarget) -> (i32, i32) {
    let (x, y) = mouse
        .drag
        .map(|drag| drag.last)
        .or(mouse.press)
        .unwrap_or((mouse.column, mouse.row));
    (i32::from(x), i32::from(y))
}

fn armed_mouse_drag(kind: MouseDragKind, pane: PaneId, mouse: &MouseEventTarget) -> MuxEffect {
    MuxEffect::ArmMouseDrag(MouseDrag {
        kind,
        pane,
        anchor: mouse_anchor(mouse),
        last: (mouse.column, mouse.row),
    })
}

impl MouseEventTarget {
    #[must_use]
    pub fn target_spelling(&self) -> Option<String> {
        self.pane
            .map(|pane| pane.to_string())
            .or_else(|| self.window.map(|window| window.to_string()))
    }
}

#[must_use]
pub fn resolve_invoking_mouse_targets(
    command: &CommandInvocation,
    mouse: &MouseEventTarget,
) -> Option<CommandInvocation> {
    mouse
        .target_spelling()
        .and_then(|target| resolve_mouse_targets(command, &target))
}

fn resolve_mouse_targets(command: &CommandInvocation, target: &str) -> Option<CommandInvocation> {
    let is_mouse_slot = |command: &CommandInvocation, index: usize| {
        index > 0
            && matches!(command.args[index - 1].as_str(), "-t" | "-s")
            && !command.argument_is_command_block(index)
            && matches!(command.args[index].as_str(), "=" | "{mouse}")
    };
    if !(0..command.args.len()).any(|index| is_mouse_slot(command, index)) {
        return None;
    }
    let mut resolved = command.clone();
    for index in 0..resolved.args.len() {
        if is_mouse_slot(&resolved, index) {
            resolved.args[index] = RawText::from(target);
        }
    }
    Some(resolved)
}

impl fmt::Debug for ExecutionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionContext")
            .field("session", &self.session)
            .field("window", &self.window)
            .field("pane", &self.pane)
            .field("client_terminal", &self.client_terminal)
            .field("client_size", &self.client_size)
            .field("client_working_directory", &self.client_working_directory)
            .field(
                "client_environment_entries",
                &self
                    .client_environment
                    .as_ref()
                    .map(|environment| environment.entries().count()),
            )
            .field("client_attached", &self.client_attached)
            .field("client_attached_context", &self.client_attached_context)
            .field(
                "target_format_client_override",
                &self.target_format_client_override,
            )
            .field("repeat_binding", &self.repeat_binding)
            .field("invoking_key", &self.invoking_key)
            .field("invoking_mouse", &self.invoking_mouse)
            .field("replay_client", &self.replay_client)
            .field("control_command_target", &self.control_command_target)
            .field("legacy_layouts", &self.legacy_layouts)
            .field("refuse_new_session_attach", &self.refuse_new_session_attach)
            .field("spawns_popup", &self.spawns_popup)
            .field("no_hooks", &self.no_hooks)
            .field("format_variables", &self.format_variables)
            .field("spawn_pane_options", &self.spawn_pane_options)
            .field("agent_parent", &self.agent_parent)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ClientTerminal {
    NoClient,
    Absent,
    #[default]
    Present,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            session: None,
            window: None,
            pane: None,
            client_terminal: ClientTerminal::Present,
            client_size: None,
            client_working_directory: None,
            client_environment: Some(Arc::default()),
            client_attached: true,
            client_attached_context: None,
            target_format_client_override: None,
            repeat_binding: false,
            invoking_key: None,
            invoking_mouse: None,
            replay_client: None,
            control_command_target: None,
            legacy_layouts: false,
            refuse_new_session_attach: false,
            spawns_popup: false,
            no_hooks: false,
            format_variables: BTreeMap::new(),
            spawn_pane_options: Vec::new(),
            agent_parent: None,
        }
    }
}

impl ExecutionContext {
    #[must_use]
    pub fn new(session: Option<SessionId>, window: Option<WindowId>, pane: Option<PaneId>) -> Self {
        Self {
            session,
            window,
            pane,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn for_pane(state: &MuxState, pane: PaneId) -> Option<Self> {
        let window = state.window_for_pane(pane)?;
        let session = state.windows.get(&window)?.session;
        Some(Self::new(Some(session), Some(window), Some(pane)))
    }

    pub fn retarget(&mut self, target: &Self) {
        self.session = target.session;
        self.window = target.window;
        self.pane = target.pane;
    }

    #[must_use]
    pub fn has_client_terminal(&self) -> bool {
        self.client_terminal == ClientTerminal::Present
    }

    #[must_use]
    pub fn has_no_client(&self) -> bool {
        self.client_terminal == ClientTerminal::NoClient
    }

    pub fn set_client_terminal(&mut self, present: bool) {
        self.client_terminal = if present {
            ClientTerminal::Present
        } else {
            ClientTerminal::Absent
        };
    }

    pub fn set_no_client(&mut self) {
        self.client_terminal = ClientTerminal::NoClient;
        self.client_environment = None;
        self.client_attached = false;
        self.client_attached_context = None;
        self.target_format_client_override = None;
    }

    pub fn set_client_attached(&mut self, attached: bool) {
        self.client_attached = attached;
        self.client_attached_context = None;
        self.target_format_client_override = None;
    }

    #[must_use]
    pub fn attached_client_context(
        &self,
    ) -> Option<(Option<SessionId>, Option<WindowId>, Option<PaneId>)> {
        self.client_attached.then(|| {
            self.client_attached_context.map_or(
                (self.session, self.window, self.pane),
                |(session, window, pane)| (Some(session), Some(window), Some(pane)),
            )
        })
    }

    pub fn set_attached_client_context(&mut self, context: Option<(SessionId, WindowId, PaneId)>) {
        self.client_attached = context.is_some();
        self.client_attached_context = context;
        self.target_format_client_override = None;
    }

    pub fn copy_client_attachment(&mut self, source: &Self) {
        self.client_attached = source.client_attached;
        self.client_attached_context = source.client_attached_context;
        self.target_format_client_override = source.target_format_client_override;
    }

    pub fn set_format_client_session(&mut self, session: Option<SessionId>) {
        self.target_format_client_override =
            Some(session.map_or(FormatClient::NoClient, FormatClient::Attached));
    }

    pub fn set_format_client(&mut self, format_client: FormatClient) {
        self.target_format_client_override = Some(format_client);
    }

    #[must_use]
    pub fn format_client(&self) -> FormatClient {
        if self.has_no_client() {
            FormatClient::NoClient
        } else {
            self.attached_client_context()
                .and_then(|(session, _, _)| session)
                .map_or(FormatClient::Unattached, FormatClient::Attached)
        }
    }

    #[must_use]
    pub fn target_format_client(&self) -> FormatClient {
        self.target_format_client_override
            .unwrap_or_else(|| self.format_client())
    }

    #[must_use]
    pub fn client_size(&self) -> Option<(u16, u16)> {
        self.client_size
    }

    pub fn set_client_size(&mut self, size: Option<(u16, u16)>) {
        self.client_size = size;
    }

    #[must_use]
    pub fn client_working_directory(&self) -> Option<&Path> {
        self.client_working_directory.as_deref()
    }

    pub fn set_client_working_directory(&mut self, working_directory: Option<PathBuf>) {
        self.client_working_directory = working_directory;
    }

    #[must_use]
    pub fn client_environment(&self) -> Option<&BTreeMap<RawText, RawText>> {
        self.client_environment
            .as_deref()
            .map(ClientEnvironmentBlob::map)
    }

    pub fn set_client_environment(&mut self, environment: Option<Arc<ClientEnvironmentBlob>>) {
        self.client_environment = environment;
    }

    #[must_use]
    pub fn repeat_binding(&self) -> bool {
        self.repeat_binding
    }

    pub fn set_repeat_binding(&mut self, repeat: bool) {
        self.repeat_binding = repeat;
    }

    #[must_use]
    pub fn invoking_key(&self) -> Option<&str> {
        self.invoking_key.as_deref()
    }

    pub fn set_invoking_key(&mut self, key: Option<String>) {
        self.invoking_key = key;
    }

    #[must_use]
    pub fn invoking_mouse(&self) -> Option<&MouseEventTarget> {
        self.invoking_mouse.as_ref()
    }

    pub fn set_invoking_mouse(&mut self, mouse: Option<MouseEventTarget>) {
        self.invoking_mouse = mouse;
    }

    #[must_use]
    pub fn replay_client(&self) -> Option<ClientId> {
        self.replay_client
    }

    pub fn set_replay_client(&mut self, client: Option<ClientId>) {
        self.replay_client = client;
    }

    #[must_use]
    pub const fn agent_parent(&self) -> Option<PaneId> {
        self.agent_parent
    }

    pub const fn set_agent_parent(&mut self, parent: Option<PaneId>) {
        self.agent_parent = parent;
    }

    #[must_use]
    pub fn control_command_target(&self) -> Option<(ClientId, u8)> {
        self.control_command_target
    }

    pub fn set_control_command_target(&mut self, target: Option<(ClientId, u8)>) {
        self.control_command_target = target;
    }

    #[must_use]
    pub fn legacy_layouts(&self) -> bool {
        self.legacy_layouts
    }

    pub fn set_legacy_layouts(&mut self, legacy: bool) {
        self.legacy_layouts = legacy;
    }

    #[must_use]
    pub const fn refuses_new_session_attach(&self) -> bool {
        self.refuse_new_session_attach
    }

    pub fn set_refuse_new_session_attach(&mut self, refuse: bool) {
        self.refuse_new_session_attach = refuse;
    }

    #[must_use]
    pub const fn spawns_popup(&self) -> bool {
        self.spawns_popup
    }

    pub fn set_spawns_popup(&mut self, popup: bool) {
        self.spawns_popup = popup;
    }

    pub fn set_spawn_pane_options(&mut self, options: Vec<(String, String)>) {
        self.spawn_pane_options = options;
    }

    pub fn retarget_to_pane(&mut self, state: &MuxState, pane: PaneId) -> bool {
        let Some(target) = Self::for_pane(state, pane) else {
            return false;
        };
        self.retarget(&target);
        true
    }

    pub fn enter_hook(&mut self, variables: BTreeMap<String, String>) {
        self.no_hooks = true;
        self.format_variables = variables;
    }

    #[must_use]
    pub fn format_variables(&self) -> Option<&BTreeMap<String, String>> {
        (!self.format_variables.is_empty()).then_some(&self.format_variables)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandPromptTemplate {
    String(String),
    Commands(Vec<CommandInvocation>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandPromptStep {
    pub label: String,
    pub input: String,
}

fn command_prompt_template_name(template: &CommandPromptTemplate) -> &str {
    match template {
        CommandPromptTemplate::Commands(commands) => commands
            .first()
            .map_or("", |command| canonical_command(&command.name)),
        CommandPromptTemplate::String(template) => {
            template.split([' ', ',']).next().unwrap_or(template)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceStream {
    Bytes(RawText),
    Spent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MuxEffect {
    PaneCreated {
        pane: PaneId,
        kind: PaneKindSnapshot,
        inherit_cwd_from: Option<PaneId>,
        cwd: Option<String>,
        command: Option<Vec<String>>,
        environment: Vec<(String, String)>,
        empty: bool,
    },
    PaneMaterialized {
        pane: PaneId,
        kind: PaneKindSnapshot,
        inherit_cwd_from: Option<PaneId>,
        cwd: Option<String>,
        command: Option<Vec<String>>,
    },
    PaneRespawned {
        pane: PaneId,
        cwd: Option<String>,
        command: Option<Vec<String>>,
        environment: Vec<(String, String)>,
        empty: bool,
    },
    AgentForked {
        pane: PaneId,
        at: Option<String>,
    },
    AgentBrief {
        pane: PaneId,
        text: String,
    },
    PaneFormatOutput {
        pane: PaneId,
        format: String,
        active_session: Option<SessionId>,
        format_client: FormatClient,
    },
    PaneWaitForExit {
        pane: PaneId,
    },
    ArmMouseDrag(MouseDrag),
    PanesRemoved(Vec<PaneId>),
    PaneRelocated {
        pane: PaneId,
        from: SessionId,
        to: SessionId,
    },
    PaneMovedInWindow {
        pane: PaneId,
    },
    ZoomCycled {
        window: WindowId,
    },
    SendKeys {
        pane: PaneId,
        keys: Vec<KeyToken>,
        repeat: u32,
    },
    SendClientKeys {
        target_client: Option<String>,
        keys: Vec<KeyToken>,
        repeat: u32,
    },
    ResetPane {
        pane: PaneId,
    },
    ArmCopyModeSource {
        pane: PaneId,
        source: PaneId,
    },
    ArmCopyModeKill {
        pane: PaneId,
    },
    CopyModeRepeat {
        pane: PaneId,
        count: u32,
        target_client: Option<String>,
        require_mode: bool,
    },
    TerminalView {
        pane: PaneId,
        action: TerminalViewAction,
        target_client: Option<String>,
        require_mode: bool,
    },
    TerminalUi {
        pane: PaneId,
        command: TerminalUiCommand,
    },
    CommandPrompt {
        steps: Vec<CommandPromptStep>,
        template: Option<CommandPromptTemplate>,
        source: Option<SourceSpan>,
        prompt_type: CommandPromptType,
        mode: CommandPromptMode,
        no_freeze: bool,
        pane: Option<PaneId>,
    },
    ChooseTree {
        pane: PaneId,
        preview: ChooserPreviewSize,
        kind: ChooseTreeKind,
        info_preview: bool,
        sessions_only: bool,
        filter: Option<String>,
        format: Option<String>,
        hide_source: bool,
        kill_source: bool,
        prompt_accept: bool,
        sort: TmuxSort,
        key_format: Option<String>,
        template: Option<String>,
        zoom: bool,
    },
    FocusSidebar {
        pane: PaneId,
    },
    ChoosePath {
        pane: PaneId,
        start_dir: Option<String>,
    },
    ChooseBuffer {
        pane: PaneId,
        preview: ChooserPreviewSize,
        filter: Option<String>,
        format: Option<String>,
        kill_source: bool,
        sort: TmuxSort,
        key_format: Option<String>,
        template: Option<String>,
        zoom: bool,
    },
    DisplayPanes {
        pane: PaneId,
        delay: Option<String>,
        selectable: bool,
        template: Option<CommandPromptTemplate>,
        source: Option<SourceSpan>,
        source_window: WindowId,
        source_session: SessionId,
        kill_source: bool,
        zoom: bool,
    },
    DisplayMessage {
        pane: Option<PaneId>,
        target_client: Option<String>,
        text: String,
        duration_ms: Option<u32>,
        freeze: bool,
        ignore_keys: bool,
    },
    PrintOrMessage {
        pane: Option<PaneId>,
        text: String,
        duration_ms: u32,
        freeze: bool,
    },
    BufferLimitChanged(usize),
    WordSeparatorsChanged {
        session: Option<SessionId>,
    },
    ModeKeysChanged {
        window: Option<WindowId>,
    },
    AggressiveResizeChanged {
        window: Option<WindowId>,
    },
    MonitorSilenceChanged,
    WindowSizeChanged {
        window: Option<WindowId>,
    },
    ResizeWindowFromClients {
        window: WindowId,
        mode: WindowSize,
    },
    TerminalKnobsChanged {
        window: Option<WindowId>,
        pane: Option<PaneId>,
    },
    ClientTerminalChanged,
    MuxOptionChanged {
        option: MuxOptionKey,
        session: Option<SessionId>,
    },
    StatusFormatsChanged {
        session: Option<SessionId>,
    },
    StatusRowsChanged {
        session: Option<SessionId>,
    },
    Attach {
        session: SessionId,
        detach_others: bool,
        detach_others_hangup: bool,
        read_only: bool,
        flags: Option<String>,
        update_environment: bool,
    },
    Detach(DetachRequest),
    SuspendClient {
        target_client: Option<String>,
    },
    SourceFile {
        path: RawText,
        quiet: bool,
        parse_only: bool,
        verbose: bool,
        context: ExecutionContext,
        stdin: Option<SourceStream>,
    },
    PaneStreamInput {
        pane: PaneId,
        bytes: RawText,
    },
    RunHook {
        name: String,
        commands: Vec<Vec<CommandInvocation>>,
        context: ExecutionContext,
    },
    ReloadConfig,
    ImportTmuxConfig(Option<String>),
    KillServer,
    SuppressAfterHook,
    SnapshotChanged,
    ModeStylesChanged,
    UserOptionChanged {
        channel: String,
    },
    PaneModeChanged {
        pane: PaneId,
        mode: Option<PaneModeRequest>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneModeRequest {
    Clock,
    Customize(Box<CustomizeMode>),
    Switch(Box<SwitchMode>),
    Panes(Box<PanesMode>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanesMode {
    pub source_session: SessionId,
    pub source_window: WindowId,
    pub duration_ms: u32,
    pub ignore_keys: bool,
    pub template: Option<CommandPromptTemplate>,
    pub source: Option<SourceSpan>,
    pub kill_source: bool,
    pub zoom: bool,
    pub token: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayPanesOptions {
    pub time_ms: u32,
    pub colour: String,
    pub active_colour: String,
    pub format: String,
    pub border_style: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetachRequest {
    pub target_client: Option<String>,
    pub scope: DetachScope,
    pub exec: Option<String>,
    pub parent_hangup: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DetachScope {
    Client,
    Others,
    Session(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Execution {
    pub output: RawText,
    pub effects: Vec<MuxEffect>,
}

struct CommandItemHooks<'a, H> {
    inner: &'a mut H,
    command: Option<&'a str>,
}

struct ListCommandHooks<'a, H> {
    inner: &'a mut H,
    spec: &'a CommandSpec,
}

struct ListKeyHooks<'a, H> {
    inner: &'a mut H,
    table: &'a str,
    key: &'a str,
    binding: &'a Binding,
    prefix: &'a str,
    notes_only: bool,
    has_repeat: bool,
    key_width: &'a str,
    table_width: &'a str,
    options: &'a StatusRowVariables,
    user_options: &'a FormatFacts,
    fallback: &'a StatusContext<'static>,
}

#[cfg(all(test, unix))]
mod key_listing_tests;

pub struct KeyListing {
    values: StatusContext<'static>,
    options: Arc<StatusRowVariables>,
    user_options: FormatFacts,
    fallback: StatusContext<'static>,
    context: ExecutionContext,
    args: Vec<RawText>,
    generation: u64,
    cacheable: bool,
    single: bool,
    had_binding: bool,
    format: String,
    bindings: Vec<OwnedListedKey>,
    next: usize,
    prefix: String,
    notes_only: bool,
    has_repeat: bool,
    key_width: String,
    table_width: String,
    output: String,
}

struct OwnedListedKey {
    table: String,
    key: String,
    binding: Binding,
}

#[derive(Debug)]
struct CachedKeyListing {
    generation: u64,
    args: Vec<RawText>,
    output: String,
    had_binding: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TmuxKeyIdentity {
    base: u32,
    key_type: u8,
    modifiers: u8,
    flags: u8,
    canonical: String,
}

impl TmuxKeyIdentity {
    fn masked_eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.key_type == other.key_type
            && self.modifiers == other.modifiers
    }
}

struct ParsedTmuxKey {
    rendered: String,
    identity: TmuxKeyIdentity,
}

struct ListedKey<'a> {
    table: &'a str,
    key: String,
    binding: &'a Binding,
    identity: TmuxKeyIdentity,
    original: usize,
}

struct ConfigConditionHooks<'a> {
    engine: &'a MuxEngine,
    inner: CommandHooks,
}

impl StatusHooks for ConfigConditionHooks<'_> {
    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
        String::new()
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        let option = if name.starts_with('@') {
            self.engine
                .server_user_options
                .get(name)
                .or_else(|| self.engine.global_window_user_options.get(name))
                .or_else(|| self.engine.global_session_user_options.get(name))
                .map(ToString::to_string)
        } else {
            self.engine.global_tmux_option_value(name)
        };
        option.or_else(|| {
            context.variable(name).is_none().then(|| {
                self.engine
                    .global_environment
                    .get(name)
                    .and_then(|entry| entry.value.as_ref().map(ToString::to_string))
            })?
        })
    }
}

impl<H: StatusHooks> StatusHooks for CommandItemHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        if !COMMAND_ITEM_CONTEXT_FORMATS.contains(&name) {
            return self.inner.tree_variable(name, context);
        }
        let command = self.command?;
        self.inner
            .tree_variable(name, context)
            .or(Some(Cow::Borrowed(command)))
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        self.inner.variable(name, context)
    }

    fn window_activity(&mut self, window: WindowId) -> u64 {
        self.inner.window_activity(window)
    }

    fn pane_activity(&mut self, pane: PaneId) -> u64 {
        self.inner.pane_activity(pane)
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }

    fn tree_entries(&mut self) -> Vec<(String, String)> {
        let mut entries = self.inner.tree_entries();
        if let Some(command) = self.command
            && !entries.iter().any(|(name, _)| name == "command")
        {
            entries.push(("command".to_owned(), command.to_owned()));
        }
        entries
    }
}

impl<H: StatusHooks> StatusHooks for ListCommandHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        if !LIST_COMMAND_CONTEXT_FORMATS.contains(&name) {
            return self.inner.tree_variable(name, context);
        }
        Some(Cow::Borrowed(match name {
            "command_list_name" => self.spec.name,
            "command_list_alias" => self.spec.aliases.first().copied().unwrap_or_default(),
            "command_list_usage" => self.spec.usage,
            _ => unreachable!(),
        }))
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        if LIST_COMMAND_CONTEXT_FORMATS.contains(&name) {
            None
        } else {
            self.inner.variable(name, context)
        }
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }
}

impl<H: StatusHooks> StatusHooks for ListKeyHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        let option_context = if context.session_id.is_empty()
            && context.window_id.is_empty()
            && context.pane_id.is_empty()
        {
            self.fallback
        } else {
            context
        };
        if let Some(value) = self.options.lookup(
            &option_context.session_id,
            &option_context.window_id,
            &option_context.pane_id,
            name,
        ) {
            return Some(value.into());
        }
        if name.starts_with('@') {
            return self
                .user_options
                .user_option_bytes(
                    &option_context.pane_id,
                    &option_context.window_id,
                    &option_context.session_id,
                    name,
                )
                .cloned();
        }
        if self.inner.only_tmux_options()
            && *LIST_KEY_FORMAT_NAMES_ARE_NOT_OPTIONS
            && (LIST_KEY_BINDING_CONTEXT_FORMATS.contains(&name)
                || LIST_KEY_SUMMARY_CONTEXT_FORMATS.contains(&name))
        {
            return None;
        }
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        if !LIST_KEY_BINDING_CONTEXT_FORMATS.contains(&name)
            && !LIST_KEY_SUMMARY_CONTEXT_FORMATS.contains(&name)
        {
            return self.inner.tree_variable(name, context);
        }
        Some(match name {
            "notes_only" => Cow::Borrowed(if self.notes_only { "1" } else { "0" }),
            "key_repeat" => Cow::Borrowed(if self.binding.repeat { "1" } else { "0" }),
            "key_note" => Cow::Borrowed(self.binding.note.as_deref().unwrap_or_default()),
            "key_prefix" => Cow::Borrowed(self.prefix),
            "key_table" => Cow::Borrowed(self.table),
            "key_string" => Cow::Borrowed(self.key),
            "key_command" => Cow::Owned(format_key_command(self.binding)),
            "key_has_repeat" => Cow::Borrowed(if self.has_repeat { "1" } else { "0" }),
            "key_string_width" => Cow::Borrowed(self.key_width),
            "key_table_width" => Cow::Borrowed(self.table_width),
            _ => unreachable!(),
        })
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        if LIST_KEY_BINDING_CONTEXT_FORMATS.contains(&name)
            || LIST_KEY_SUMMARY_CONTEXT_FORMATS.contains(&name)
        {
            None
        } else {
            self.inner.variable(name, context)
        }
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }
}

impl Execution {
    fn output(output: impl Into<RawText>) -> Self {
        Self {
            output: output.into(),
            effects: Vec::new(),
        }
    }

    fn effect(effect: MuxEffect) -> Self {
        Self {
            output: RawText::default(),
            effects: vec![effect],
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ModeKeys {
    Vi,
    #[default]
    Emacs,
}

impl ModeKeys {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Vi => "vi",
            Self::Emacs => "emacs",
        }
    }

    const fn table(self) -> &'static str {
        match self {
            Self::Vi => "copy-mode-vi",
            Self::Emacs => "copy-mode",
        }
    }

    const fn toggled(self) -> Self {
        match self {
            Self::Vi => Self::Emacs,
            Self::Emacs => Self::Vi,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SetClipboard {
    On,
    #[default]
    External,
    Off,
}

impl SetClipboard {
    const fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::External => "external",
            Self::Off => "off",
        }
    }

    const fn toggled(self) -> Self {
        match self {
            Self::Off => Self::External,
            Self::External => Self::Off,
            Self::On => Self::On,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TmuxOptionTarget {
    Server,
    GlobalSession,
    Session(SessionId),
    GlobalWindow,
    Window(WindowId),
    Pane(PaneId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatMonitorScope {
    Session,
    Pane(PaneId),
    AllPanes,
    Window(WindowId),
    AllWindows,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct FormatMonitorTarget {
    pub session: Option<SessionId>,
    pub window: Option<WindowId>,
    pub pane: Option<PaneId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatMonitor {
    pub id: u64,
    pub name: String,
    pub scope: FormatMonitorScope,
    pub format: String,
    pub session: Option<SessionId>,
}

#[derive(Debug)]
struct FormatMonitorEntry {
    id: u64,
    name: String,
    target: TmuxOptionTarget,
    scope: FormatMonitorScope,
    format: String,
    session: Option<SessionId>,
    notify_true: bool,
    previous: BTreeMap<FormatMonitorTarget, String>,
    fire: HookFire,
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct HookFire {
    count: u32,
    time: u64,
}

impl HookFire {
    const fn fired(&mut self, now: u64) {
        self.count = self.count.saturating_add(1);
        self.time = now;
    }

    fn context(self) -> Vec<(&'static str, String)> {
        let mut values = vec![("hook_fire_count", self.count.to_string())];
        if self.time != 0 {
            values.push(("hook_fire_time", self.time.to_string()));
        }
        values
    }
}

#[derive(Clone, Copy)]
enum ShowOptionArgument {
    Expand,
    HookAlreadyExpanded,
}

type UserOptions = BTreeMap<String, RawText>;

type SharedUserOptions = Arc<UserOptions>;

#[derive(Clone, Debug, Default)]
pub struct FormatFacts {
    pane_kinds: BTreeMap<String, &'static str>,
    browser_urls: BTreeMap<String, String>,
    agent_chats: BTreeMap<String, AgentChatFacts>,
    agent_parents: BTreeMap<String, String>,
    pane_windows: BTreeMap<String, String>,
    server: SharedUserOptions,
    global_session: SharedUserOptions,
    sessions: BTreeMap<String, SharedUserOptions>,
    global_window: SharedUserOptions,
    windows: BTreeMap<String, SharedUserOptions>,
    panes: BTreeMap<String, SharedUserOptions>,
}

impl FormatFacts {
    #[must_use]
    pub fn pane_kind(&self, pane: &str) -> Option<&'static str> {
        self.pane_kinds.get(pane).copied()
    }

    #[must_use]
    pub fn browser_url(&self, pane: &str) -> Option<&str> {
        self.browser_urls.get(pane).map(String::as_str)
    }

    #[must_use]
    pub fn agent_chat(&self, pane: &str) -> Option<&AgentChatFacts> {
        self.agent_chats.get(pane)
    }

    #[must_use]
    pub fn agent_parent(&self, pane: &str) -> Option<&str> {
        self.agent_parents.get(pane).map(String::as_str)
    }

    #[must_use]
    pub fn user_option(&self, pane: &str, window: &str, session: &str, name: &str) -> Option<&str> {
        self.user_option_bytes(pane, window, session, name)
            .map(RawText::as_str)
    }

    pub fn user_option_bytes(
        &self,
        pane: &str,
        window: &str,
        session: &str,
        name: &str,
    ) -> Option<&RawText> {
        let window = self.pane_windows.get(pane).map_or(window, String::as_str);
        self.panes
            .get(pane)
            .and_then(|values| values.get(name))
            .or_else(|| self.windows.get(window).and_then(|values| values.get(name)))
            .or_else(|| {
                self.sessions
                    .get(session)
                    .and_then(|values| values.get(name))
            })
            .or_else(|| self.global_window.get(name))
            .or_else(|| self.global_session.get(name))
            .or_else(|| self.server.get(name))
    }
}

fn pane_kind_name(kind: &PaneKind) -> &'static str {
    match kind {
        PaneKind::Picker { .. } => "picker",
        PaneKind::Terminal => "terminal",
        PaneKind::Browser(_) => "browser",
        PaneKind::Editor(_) => "editor",
        PaneKind::Agent(_) => "agent",
        PaneKind::View(_) => "view",
        PaneKind::Simulator(_) => SIMULATOR_PANE_KIND,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ArrayIndex {
    Numeric(u32),
    Named(String),
}

impl ArrayIndex {
    fn parse(value: String) -> Self {
        value
            .parse()
            .map_or_else(|_| Self::Named(value), Self::Numeric)
    }

    fn display(&self) -> String {
        match self {
            Self::Numeric(index) => index.to_string(),
            Self::Named(index) => index.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FormatOptionIndex {
    Whole,
    Indexed(ArrayIndex),
    Invalid,
}

fn parse_session_id(value: &str) -> Option<SessionId> {
    value.strip_prefix('$')?.parse().ok().map(SessionId)
}

fn parse_window_id(value: &str) -> Option<WindowId> {
    value.strip_prefix('@')?.parse().ok().map(WindowId)
}

fn parse_pane_id(value: &str) -> Option<PaneId> {
    value.strip_prefix('%')?.parse().ok().map(PaneId)
}

type StringArray = BTreeMap<ArrayIndex, String>;
type ArrayTable = BTreeMap<&'static str, StringArray>;
type HookArray = BTreeMap<ArrayIndex, Vec<CommandInvocation>>;
type HookTable = BTreeMap<String, HookArray>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredArrays {
    server: ArrayTable,
    global_session: ArrayTable,
    sessions: BTreeMap<SessionId, ArrayTable>,
    global_window: ArrayTable,
    windows: BTreeMap<WindowId, ArrayTable>,
    panes: BTreeMap<PaneId, ArrayTable>,
}

type ScalarTable = BTreeMap<&'static str, String>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct StoredScalars {
    server: ScalarTable,
    global_session: ScalarTable,
    sessions: BTreeMap<SessionId, ScalarTable>,
    global_window: ScalarTable,
    windows: BTreeMap<WindowId, ScalarTable>,
    panes: BTreeMap<PaneId, ScalarTable>,
}

impl Default for StoredArrays {
    fn default() -> Self {
        Self {
            server: default_array_table(TmuxOptionScope::Server),
            global_session: default_array_table(TmuxOptionScope::Session),
            sessions: BTreeMap::new(),
            global_window: default_array_table(TmuxOptionScope::WindowPane),
            windows: BTreeMap::new(),
            panes: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EnvironmentEntry {
    value: Option<RawText>,
    hidden: bool,
}

type Environment = BTreeMap<RawText, EnvironmentEntry>;

#[derive(Clone, Debug, Default)]
pub struct RetainedJobEnvironment {
    inner: Arc<Mutex<Environment>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RemainOnExit {
    #[default]
    Off,
    On,
    Failed,
    Key,
    FailedKey,
}

impl RemainOnExit {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Failed => "failed",
            Self::Key => "key",
            Self::FailedKey => "failed-key",
        }
    }

    const fn toggled(self) -> Self {
        match self {
            Self::Off => Self::On,
            Self::On => Self::Off,
            other => other,
        }
    }
}

static EMPTY_ENVIRONMENT: Environment = Environment::new();

const NATIVE_OPTIONS: &[&str] = &[
    "history-trickle",
    "experimental-editor-pane",
    "agent-command",
    "agent-claude-code-command",
    "agent-auto-approve",
];

const COMMAND_ALIAS_GROUP_NAME: &str = "__zz-command-alias-group";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandAliasBodyError {
    Unparsable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandAliasResolution {
    Miss,
    Expanded(CommandInvocation),
    MatchedUnsupported(CommandAliasBodyError),
}

impl CommandAliasResolution {
    pub fn into_command(self, typed: &CommandInvocation) -> Result<CommandInvocation, ServerError> {
        match self {
            Self::Miss => Ok(typed.clone()),
            Self::Expanded(command) => Ok(command),
            Self::MatchedUnsupported(_) => Err(ServerError::CommandParse(format!(
                "unknown command: {}",
                typed.name
            ))),
        }
    }
}

#[derive(Debug)]
pub struct MuxEngine {
    pub state: MuxState,
    pane_status_hidden: BTreeSet<PaneId>,
    swept_removals: u64,
    pub keys: KeyTables,
    global_mode_keys: ModeKeys,
    window_mode_keys: BTreeMap<WindowId, ModeKeys>,
    set_clipboard: SetClipboard,
    copy_command: String,
    buffer_limit: usize,
    message_limit: usize,
    history_trickle: usize,
    global_history_limit: usize,
    session_history_limits: BTreeMap<SessionId, usize>,
    global_base_index: u32,
    session_base_indices: BTreeMap<SessionId, u32>,
    global_renumber_windows: bool,
    session_renumber_windows: BTreeMap<SessionId, bool>,
    global_word_separators: String,
    session_word_separators: BTreeMap<SessionId, String>,
    global_mouse: bool,
    session_mouse: BTreeMap<SessionId, bool>,
    escape_time_ms: u32,
    global_automatic_rename_format: String,
    window_automatic_rename_formats: BTreeMap<WindowId, String>,
    global_remain_on_exit: RemainOnExit,
    window_remain_on_exit: BTreeMap<WindowId, RemainOnExit>,
    pane_remain_on_exit: BTreeMap<PaneId, RemainOnExit>,
    global_popup_style: String,
    window_popup_styles: BTreeMap<WindowId, String>,
    global_popup_border_style: String,
    window_popup_border_styles: BTreeMap<WindowId, String>,
    global_popup_border_lines: PopupBorderLines,
    window_popup_border_lines: BTreeMap<WindowId, PopupBorderLines>,
    global_menu_style: String,
    window_menu_styles: BTreeMap<WindowId, String>,
    global_menu_selected_style: String,
    window_menu_selected_styles: BTreeMap<WindowId, String>,
    global_menu_border_style: String,
    window_menu_border_styles: BTreeMap<WindowId, String>,
    global_menu_border_lines: PopupBorderLines,
    window_menu_border_lines: BTreeMap<WindowId, PopupBorderLines>,
    global_lock_command: String,
    session_lock_commands: BTreeMap<SessionId, String>,
    global_lock_after_time: u32,
    session_lock_after_times: BTreeMap<SessionId, u32>,
    global_default_command: String,
    session_default_commands: BTreeMap<SessionId, String>,
    global_default_shell: String,
    session_default_shells: BTreeMap<SessionId, String>,
    default_terminal: Option<String>,
    global_display_time_ms: u32,
    session_display_time_ms: BTreeMap<SessionId, u32>,
    global_initial_repeat_time_ms: u32,
    session_initial_repeat_time_ms: BTreeMap<SessionId, u32>,
    global_repeat_time_ms: u32,
    session_repeat_time_ms: BTreeMap<SessionId, u32>,
    server_user_options: SharedUserOptions,
    global_session_user_options: SharedUserOptions,
    session_user_options: BTreeMap<SessionId, SharedUserOptions>,
    global_window_user_options: SharedUserOptions,
    window_user_options: BTreeMap<WindowId, SharedUserOptions>,
    pane_user_options: BTreeMap<PaneId, SharedUserOptions>,
    stored_arrays: StoredArrays,
    stored_scalars: StoredScalars,
    global_hooks: HookTable,
    session_hooks: BTreeMap<SessionId, HookTable>,
    global_window_hooks: HookTable,
    window_hooks: BTreeMap<WindowId, HookTable>,
    pane_hooks: BTreeMap<PaneId, HookTable>,
    hook_events: BTreeSet<String>,
    global_environment: Environment,
    session_environments: BTreeMap<SessionId, RetainedJobEnvironment>,
    status: StatusFormats,
    session_status_options: BTreeMap<SessionId, BTreeMap<StatusOption, String>>,
    explicit_status_options: BTreeSet<&'static str>,
    session_explicit_status_options: BTreeMap<SessionId, BTreeSet<&'static str>>,
    window_status: WindowStatusFormats,
    window_status_options: BTreeMap<WindowId, BTreeMap<WindowStatusOption, String>>,
    server_options: ServerOptions,
    global_session_options: SessionOptions,
    session_options: BTreeMap<SessionId, BTreeMap<SessionOption, String>>,
    global_window_options: WindowOptions,
    window_options: BTreeMap<WindowId, BTreeMap<WindowOption, String>>,
    global_pane_options: PaneOptions,
    window_pane_options: BTreeMap<WindowId, BTreeMap<PaneOption, String>>,
    pane_options: BTreeMap<PaneId, BTreeMap<PaneOption, String>>,
    format_host: String,
    format_host_short: String,
    format_pid: u32,
    format_socket_path: String,
    format_start_time: u64,
    format_uid: String,
    format_user: String,
    pane_runtime_facts: BTreeMap<PaneId, PaneRuntimeFacts>,
    agent_chats: BTreeMap<PaneId, AgentChatFacts>,
    agent_parents: BTreeMap<PaneId, PaneId>,
    pane_start_commands: BTreeMap<PaneId, Vec<String>>,
    pane_shells: BTreeMap<PaneId, String>,
    destroyed_sessions: Vec<(SessionId, String, String)>,
    experimental_editor_pane: bool,
    agent: AgentOptions,
    format_monitors: Vec<FormatMonitorEntry>,
    next_format_monitor_id: u64,
    hook_fires: BTreeMap<(TmuxOptionTarget, String), HookFire>,
    automatic_rename_throttle: bool,
    window_name_times: BTreeMap<WindowId, Instant>,
    pending_window_renames: BTreeSet<WindowId>,
    format_options_generation: u64,
    format_option_cache: Mutex<Option<(u64, u64, Arc<StatusRowVariables>)>>,
    format_needs_cache: Mutex<BTreeMap<u64, (u64, Vec<String>, crate::FormatNeeds)>>,
    pub(crate) format_reference_cache:
        Mutex<Option<(u64, BTreeMap<String, Arc<BTreeSet<String>>>)>>,
    key_listing_cache: Mutex<Option<CachedKeyListing>>,
    pub(crate) format_data_generation: u64,
    pub(crate) format_context_cache: Mutex<Option<crate::formats::DetachedContextCache>>,
    pub(crate) format_reference_union_cache:
        Mutex<Option<crate::formats::FormatReferenceUnionCache>>,
    format_cache_identity: Arc<()>,
}

const NAME_INTERVAL: Duration = Duration::from_millis(500);

const NEW_SESSION_GROUP_REFUSAL: &str = "new-session -t (zz has no session groups; use attach -t <session>, every client keeps its own current window)";

const RUNTIME_FACT_MARKERS: [&str; 11] = [
    "pane_current_command",
    "pane_current_path",
    "pane_path",
    "pane_pid",
    "pane_tty",
    "pane_start_path",
    "pane_dead_signal",
    "@",
    "E:",
    "T:",
    "O:",
];

const CLOCK_MARKERS: [&str; 5] = ["#(", "t:", "t/", "E:", "T:"];

fn template_reads_runtime_facts(template: &str) -> bool {
    template.contains('#')
        && RUNTIME_FACT_MARKERS
            .iter()
            .any(|marker| template.contains(marker))
}

#[cfg(test)]
mod runtime_facts_tests;

#[cfg(test)]
mod format_cache_tests;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentChatFacts {
    pub state: &'static str,
    pub pending_permission: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaneRuntimeFacts {
    pub current_command: String,
    pub current_path: String,
    pub dead_signal: String,
    pub reported_path: String,
    pub start_path: String,
    pub pid: Option<u32>,
    pub tty: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalWorkerOptions {
    pub allow_passthrough: bool,
    pub allow_rename: bool,
    pub alternate_screen: bool,
    pub scroll_on_clear: bool,
    pub erase_byte: Option<u8>,
    pub verase_byte: u8,
    pub mode_keys_vi: bool,
    pub remain_on_exit_format: String,
    pub wrap_search: bool,
    pub cursor_style: &'static str,
    pub cursor_colour: String,
    pub window_style: Option<String>,
    pub window_active_style: Option<String>,
    pub active_pane: bool,
    pub pane_colours: Vec<(u8, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaneBorderStyleValues {
    pub border: Option<String>,
    pub active_border: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowStyleValues {
    pub style: Option<String>,
    pub active_style: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyModeStyleValues {
    pub mode: Option<String>,
    pub search_match: Option<String>,
    pub search_current_match: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusRowVariables {
    pub base: BTreeMap<String, String>,
    pub sessions: BTreeMap<String, BTreeMap<String, String>>,
    pub windows: BTreeMap<String, BTreeMap<String, String>>,
    pub panes: BTreeMap<String, BTreeMap<String, String>>,
    session_active_windows: BTreeMap<String, String>,
    window_active_panes: BTreeMap<String, String>,
    pane_windows: BTreeMap<String, String>,
    window_sessions: BTreeMap<String, String>,
}

impl StatusRowVariables {
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        fn values_bytes(values: &BTreeMap<String, String>) -> usize {
            values.iter().fold(0usize, |total, (name, value)| {
                total
                    .saturating_add(std::mem::size_of::<(String, String)>())
                    .saturating_add(name.capacity())
                    .saturating_add(value.capacity())
            })
        }
        let scoped = [&self.sessions, &self.windows, &self.panes]
            .into_iter()
            .flat_map(BTreeMap::iter)
            .fold(0usize, |total, (scope, values)| {
                total
                    .saturating_add(std::mem::size_of::<(String, BTreeMap<String, String>)>())
                    .saturating_add(scope.capacity())
                    .saturating_add(values_bytes(values))
            });
        [
            &self.base,
            &self.session_active_windows,
            &self.window_active_panes,
            &self.pane_windows,
            &self.window_sessions,
        ]
        .into_iter()
        .fold(
            std::mem::size_of::<Self>().saturating_add(scoped),
            |total, values| total.saturating_add(values_bytes(values)),
        )
    }

    #[must_use]
    pub fn lookup(
        &self,
        session_id: &str,
        window_id: &str,
        pane_id: &str,
        name: &str,
    ) -> Option<String> {
        let (option, index) = parse_format_option(name)?;
        let pane = self
            .panes
            .contains_key(pane_id)
            .then_some(pane_id)
            .or_else(|| self.window_active_panes.get(window_id).map(String::as_str))
            .or_else(|| {
                self.session_active_windows
                    .get(session_id)
                    .and_then(|window| self.window_active_panes.get(window))
                    .map(String::as_str)
            });
        let window = pane
            .and_then(|pane| self.pane_windows.get(pane).map(String::as_str))
            .or_else(|| self.windows.contains_key(window_id).then_some(window_id))
            .or_else(|| {
                self.session_active_windows
                    .get(session_id)
                    .map(String::as_str)
            });
        let session = window
            .and_then(|window| self.window_sessions.get(window).map(String::as_str))
            .or_else(|| self.sessions.contains_key(session_id).then_some(session_id));
        let values = match option.scope {
            TmuxOptionScope::Server => &self.base,
            TmuxOptionScope::Session => session
                .and_then(|session| self.sessions.get(session))
                .unwrap_or(&self.base),
            TmuxOptionScope::Window => window
                .and_then(|window| self.windows.get(window))
                .unwrap_or(&self.base),
            TmuxOptionScope::WindowPane => pane
                .and_then(|pane| self.panes.get(pane))
                .unwrap_or(&self.base),
        };
        format_option_map_value(values, option.name, &index)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentOptions {
    pub command: String,
    pub claude_code_command: String,
    pub auto_approve: AgentAutoApprove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PopupOptions {
    pub style: String,
    pub border_style: String,
    pub border_lines: PopupBorderLines,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuOptions {
    pub style: String,
    pub selected_style: String,
    pub border_style: String,
    pub border_lines: PopupBorderLines,
}

impl Default for PopupOptions {
    fn default() -> Self {
        Self {
            style: "bg=themedarkgrey,fg=themewhite".to_owned(),
            border_style: "bg=themedarkgrey,fg=themelightgrey".to_owned(),
            border_lines: PopupBorderLines::Single,
        }
    }
}

impl Default for MenuOptions {
    fn default() -> Self {
        Self {
            style: "bg=themedarkgrey,fg=themewhite".to_owned(),
            selected_style: "bg=themeyellow,fg=themeblack".to_owned(),
            border_style: "bg=themedarkgrey,fg=themelightgrey".to_owned(),
            border_lines: PopupBorderLines::Single,
        }
    }
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            command: DEFAULT_AGENT_COMMAND.to_owned(),
            claude_code_command: DEFAULT_AGENT_CLAUDE_CODE_COMMAND.to_owned(),
            auto_approve: DEFAULT_AGENT_AUTO_APPROVE,
        }
    }
}

impl Default for MuxEngine {
    fn default() -> Self {
        Self {
            state: MuxState::default(),
            pane_status_hidden: BTreeSet::new(),
            swept_removals: 0,
            keys: KeyTables::default(),
            global_mode_keys: ModeKeys::default(),
            window_mode_keys: BTreeMap::new(),
            set_clipboard: SetClipboard::default(),
            copy_command: String::new(),
            buffer_limit: DEFAULT_BUFFER_LIMIT,
            message_limit: DEFAULT_MESSAGE_LIMIT,
            history_trickle: DEFAULT_HISTORY_TRICKLE,
            global_history_limit: DEFAULT_HISTORY_LIMIT,
            session_history_limits: BTreeMap::new(),
            global_base_index: DEFAULT_BASE_INDEX,
            session_base_indices: BTreeMap::new(),
            global_renumber_windows: DEFAULT_RENUMBER_WINDOWS,
            session_renumber_windows: BTreeMap::new(),
            global_word_separators: DEFAULT_WORD_SEPARATORS.to_owned(),
            session_word_separators: BTreeMap::new(),
            global_mouse: DEFAULT_MOUSE,
            session_mouse: BTreeMap::new(),
            escape_time_ms: DEFAULT_ESCAPE_TIME_MS,
            global_automatic_rename_format: DEFAULT_AUTOMATIC_RENAME_FORMAT.to_owned(),
            window_automatic_rename_formats: BTreeMap::new(),
            global_remain_on_exit: RemainOnExit::default(),
            window_remain_on_exit: BTreeMap::new(),
            pane_remain_on_exit: BTreeMap::new(),
            global_popup_style: PopupOptions::default().style,
            window_popup_styles: BTreeMap::new(),
            global_popup_border_style: PopupOptions::default().border_style,
            window_popup_border_styles: BTreeMap::new(),
            global_popup_border_lines: PopupBorderLines::Single,
            window_popup_border_lines: BTreeMap::new(),
            global_menu_style: MenuOptions::default().style,
            window_menu_styles: BTreeMap::new(),
            global_menu_selected_style: MenuOptions::default().selected_style,
            window_menu_selected_styles: BTreeMap::new(),
            global_menu_border_style: MenuOptions::default().border_style,
            window_menu_border_styles: BTreeMap::new(),
            global_menu_border_lines: PopupBorderLines::Single,
            window_menu_border_lines: BTreeMap::new(),
            global_lock_command: "lock -np".to_owned(),
            session_lock_commands: BTreeMap::new(),
            global_lock_after_time: 0,
            session_lock_after_times: BTreeMap::new(),
            global_default_command: String::new(),
            session_default_commands: BTreeMap::new(),
            global_default_shell: DEFAULT_SHELL.to_owned(),
            session_default_shells: BTreeMap::new(),
            default_terminal: None,
            global_display_time_ms: DEFAULT_DISPLAY_TIME_MS,
            session_display_time_ms: BTreeMap::new(),
            global_initial_repeat_time_ms: DEFAULT_INITIAL_REPEAT_TIME_MS,
            session_initial_repeat_time_ms: BTreeMap::new(),
            global_repeat_time_ms: DEFAULT_REPEAT_TIME_MS,
            session_repeat_time_ms: BTreeMap::new(),
            server_user_options: SharedUserOptions::default(),
            global_session_user_options: SharedUserOptions::default(),
            session_user_options: BTreeMap::new(),
            global_window_user_options: SharedUserOptions::default(),
            window_user_options: BTreeMap::new(),
            pane_user_options: BTreeMap::new(),
            stored_arrays: StoredArrays::default(),
            stored_scalars: StoredScalars::default(),
            global_hooks: global_hook_table(TmuxOptionScope::Session),
            session_hooks: BTreeMap::new(),
            global_window_hooks: global_hook_table(TmuxOptionScope::Window),
            window_hooks: BTreeMap::new(),
            pane_hooks: BTreeMap::new(),
            hook_events: BTreeSet::new(),
            global_environment: Environment::new(),
            session_environments: BTreeMap::new(),
            status: StatusFormats::default(),
            session_status_options: BTreeMap::new(),
            explicit_status_options: BTreeSet::new(),
            session_explicit_status_options: BTreeMap::new(),
            window_status: WindowStatusFormats::default(),
            window_status_options: BTreeMap::new(),
            server_options: ServerOptions::default(),
            global_session_options: SessionOptions::default(),
            session_options: BTreeMap::new(),
            global_window_options: WindowOptions::default(),
            window_options: BTreeMap::new(),
            global_pane_options: PaneOptions::default(),
            window_pane_options: BTreeMap::new(),
            pane_options: BTreeMap::new(),
            format_host: String::new(),
            format_host_short: String::new(),
            format_pid: 0,
            format_socket_path: String::new(),
            format_start_time: 0,
            format_uid: String::new(),
            format_user: String::new(),
            pane_runtime_facts: BTreeMap::new(),
            agent_chats: BTreeMap::new(),
            agent_parents: BTreeMap::new(),
            pane_start_commands: BTreeMap::new(),
            pane_shells: BTreeMap::new(),
            destroyed_sessions: Vec::new(),
            experimental_editor_pane: false,
            agent: AgentOptions::default(),
            format_monitors: Vec::new(),
            next_format_monitor_id: 0,
            hook_fires: BTreeMap::new(),
            automatic_rename_throttle: false,
            window_name_times: BTreeMap::new(),
            pending_window_renames: BTreeSet::new(),
            format_options_generation: 0,
            format_option_cache: Mutex::new(None),
            format_needs_cache: Mutex::new(BTreeMap::new()),
            format_reference_cache: Mutex::new(None),
            key_listing_cache: Mutex::new(None),
            format_data_generation: 0,
            format_context_cache: Mutex::new(None),
            format_reference_union_cache: Mutex::new(None),
            format_cache_identity: Arc::new(()),
        }
    }
}

impl MuxEngine {
    pub fn set_format_server_context(
        &mut self,
        host: impl Into<String>,
        host_short: impl Into<String>,
        socket_path: impl Into<String>,
        start_time: u64,
    ) {
        self.format_host = host.into();
        self.format_host_short = host_short.into();
        self.format_socket_path = socket_path.into();
        self.format_start_time = start_time;
        self.state.set_default_pane_title(self.format_host.clone());
        self.state.set_format_now(start_time);
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    pub const fn set_format_now(&mut self, now: u64) {
        self.state.set_format_now(now);
    }

    pub fn expand_pane_format_time(
        &self,
        format: &str,
        target: &ExecutionContext,
        active_session: Option<SessionId>,
        format_client: FormatClient,
        hooks: &mut impl StatusHooks,
    ) -> String {
        expand_format_time_with_hooks(
            format,
            self,
            FormatContext {
                session: target.session,
                window: target.window,
                pane: target.pane,
                active_session,
                format_client,
                format_type: FormatType::Pane,
            },
            hooks,
        )
        .into()
    }

    pub fn expand_pane_format(
        &self,
        format: &str,
        target: &ExecutionContext,
        active_session: Option<SessionId>,
        format_client: FormatClient,
        hooks: &mut impl StatusHooks,
    ) -> String {
        self.expand_pane_format_bytes(format, target, active_session, format_client, hooks)
            .into()
    }

    pub fn expand_target_format(
        &self,
        format: &str,
        target: &ExecutionContext,
        active_session: Option<SessionId>,
        format_client: FormatClient,
        hooks: &mut impl StatusHooks,
    ) -> String {
        let format_type = if target.pane.is_some() {
            FormatType::Pane
        } else if target.window.is_some() {
            FormatType::Window
        } else if target.session.is_some() {
            FormatType::Session
        } else {
            FormatType::None
        };
        expand_format_with_hooks(
            format,
            self,
            FormatContext {
                session: target.session,
                window: target.window,
                pane: target.pane,
                active_session,
                format_client,
                format_type,
            },
            hooks,
        )
        .into()
    }

    pub fn expand_pane_format_bytes(
        &self,
        format: &str,
        target: &ExecutionContext,
        active_session: Option<SessionId>,
        format_client: FormatClient,
        hooks: &mut impl StatusHooks,
    ) -> RawText {
        expand_format_with_hooks(
            format,
            self,
            FormatContext {
                session: target.session,
                window: target.window,
                pane: target.pane,
                active_session,
                format_client,
                format_type: FormatType::Pane,
            },
            hooks,
        )
    }

    pub fn set_format_server_identity(
        &mut self,
        pid: u32,
        uid: impl Into<String>,
        user: impl Into<String>,
    ) {
        self.format_pid = pid;
        self.format_uid = uid.into();
        self.format_user = user.into();
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    pub fn seed_global_environment<I, K, V>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<RawText>,
        V: Into<RawText>,
    {
        self.global_environment = entries
            .into_iter()
            .map(|(name, value)| {
                (
                    name.into(),
                    EnvironmentEntry {
                        value: Some(value.into()),
                        hidden: false,
                    },
                )
            })
            .collect();
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    #[must_use]
    pub fn global_environment_variable(&self, name: &str) -> Option<String> {
        self.global_environment_bytes(name).map(String::from)
    }

    #[must_use]
    pub fn global_environment_bytes(&self, name: &str) -> Option<RawText> {
        self.global_environment
            .get(name)
            .and_then(|entry| entry.value.clone())
    }

    pub fn set_config_environment(
        &mut self,
        name: String,
        value: impl Into<RawText>,
        hidden: bool,
    ) {
        self.global_environment.insert(
            name.into(),
            EnvironmentEntry {
                value: Some(value.into()),
                hidden,
            },
        );
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    pub fn mark_session_active_at(&mut self, session: SessionId, now: u64) {
        let before = self
            .state
            .sessions
            .get(&session)
            .map(|session| (session.activity, session.sort_activity));
        self.state.mark_session_active_at(session, now);
        let after = self
            .state
            .sessions
            .get(&session)
            .map(|session| (session.activity, session.sort_activity));
        if before != after {
            self.format_data_generation = self.format_data_generation.wrapping_add(1);
        }
    }

    pub fn touch_window_activity_for_pane(&mut self, pane: PaneId) {
        let window = self.state.window_for_pane(pane);
        let before = window
            .and_then(|window| self.state.windows.get(&window))
            .map(|window| (window.activity, window.activity_time));
        self.state.touch_window_activity_for_pane(pane);
        let after = window
            .and_then(|window| self.state.windows.get(&window))
            .map(|window| (window.activity, window.activity_time));
        if before != after {
            self.format_data_generation = self.format_data_generation.wrapping_add(1);
        }
    }

    pub fn set_pane_runtime_facts(&mut self, pane: PaneId, facts: PaneRuntimeFacts) -> bool {
        let mut hooks = CommandHooks::new(self.format_now());
        self.set_pane_runtime_facts_with_hooks(pane, facts, &mut hooks)
    }

    pub fn set_pane_runtime_facts_with_hooks(
        &mut self,
        pane: PaneId,
        facts: PaneRuntimeFacts,
        hooks: &mut impl StatusHooks,
    ) -> bool {
        self.set_pane_runtime_facts_at(pane, facts, hooks, Instant::now())
    }

    pub fn set_pane_runtime_facts_at(
        &mut self,
        pane: PaneId,
        mut facts: PaneRuntimeFacts,
        hooks: &mut impl StatusHooks,
        now: Instant,
    ) -> bool {
        if self.state.pane(pane).is_none() {
            return false;
        }
        facts.dead_signal = tmux_signal_name(&facts.dead_signal);
        let command_changed = self
            .pane_runtime_facts
            .get(&pane)
            .map_or(!facts.current_command.is_empty(), |previous| {
                previous.current_command != facts.current_command
            });
        if self.pane_runtime_facts.get(&pane) == Some(&facts) {
            return false;
        }
        self.pane_runtime_facts.insert(pane, facts);
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
        if command_changed {
            self.refresh_automatic_window_name_throttled(pane, hooks, now);
        }
        true
    }

    pub fn set_automatic_rename_throttle(&mut self, enabled: bool) {
        self.automatic_rename_throttle = enabled;
        if !enabled {
            self.pending_window_renames.clear();
        }
    }

    fn refresh_automatic_window_name_throttled(
        &mut self,
        pane: PaneId,
        hooks: &mut impl StatusHooks,
        now: Instant,
    ) -> bool {
        if !self.automatic_rename_throttle {
            return self.refresh_automatic_window_name_for_pane(pane, hooks);
        }
        let Some(window) = self.automatic_rename_window(pane) else {
            return false;
        };
        if self.window_name_waits(window, now) {
            self.pending_window_renames.insert(window);
            return false;
        }
        self.window_name_times.insert(window, now);
        self.pending_window_renames.remove(&window);
        self.refresh_automatic_window_name_for_pane(pane, hooks)
    }

    fn automatic_rename_window(&self, pane: PaneId) -> Option<WindowId> {
        let window = self.state.window_for_pane(pane)?;
        (self.state.windows[&window].active_pane == pane
            && self
                .state
                .window_automatic_rename(window)
                .unwrap_or_default())
        .then_some(window)
    }

    fn window_name_waits(&self, window: WindowId, now: Instant) -> bool {
        self.window_name_times
            .get(&window)
            .is_some_and(|last| now.saturating_duration_since(*last) < NAME_INTERVAL)
    }

    #[must_use]
    pub fn automatic_rename_due(&self, pane: PaneId, now: Instant) -> bool {
        self.automatic_rename_window(pane).is_some_and(|window| {
            !self.automatic_rename_throttle || !self.window_name_waits(window, now)
        })
    }

    #[must_use]
    pub fn next_window_rename_deadline(&self) -> Option<Instant> {
        self.pending_window_renames
            .iter()
            .filter_map(|window| self.window_name_times.get(window))
            .map(|last| *last + NAME_INTERVAL)
            .min()
    }

    pub fn note_automatic_rename_output(&mut self, pane: PaneId, now: Instant) {
        if !self.automatic_rename_throttle {
            return;
        }
        let Some(window) = self.automatic_rename_window(pane) else {
            return;
        };
        match self.window_name_times.get(&window) {
            Some(last) if *last == now => {}
            Some(last) if now.saturating_duration_since(*last) < NAME_INTERVAL => {
                self.pending_window_renames.insert(window);
            }
            _ => {
                self.window_name_times.insert(window, now);
            }
        }
    }

    #[must_use]
    pub fn due_window_rename_panes(&self, now: Instant) -> Vec<PaneId> {
        self.pending_window_renames
            .iter()
            .filter(|window| !self.window_name_waits(**window, now))
            .filter_map(|window| self.state.windows.get(window))
            .map(|window| window.active_pane)
            .collect()
    }

    pub fn apply_due_window_renames(&mut self, now: Instant, hooks: &mut impl StatusHooks) -> bool {
        self.window_name_times
            .retain(|window, _| self.state.windows.contains_key(window));
        let due = self
            .pending_window_renames
            .iter()
            .copied()
            .filter(|window| {
                self.window_name_times
                    .get(window)
                    .is_none_or(|last| now.saturating_duration_since(*last) >= NAME_INTERVAL)
            })
            .collect::<Vec<_>>();
        let mut renamed = false;
        for window in due {
            self.pending_window_renames.remove(&window);
            let Some(pane) = self
                .state
                .windows
                .get(&window)
                .map(|state| state.active_pane)
            else {
                continue;
            };
            self.window_name_times.insert(window, now);
            renamed |= self.refresh_automatic_window_name_for_pane(pane, hooks);
        }
        renamed
    }

    #[must_use]
    pub fn runtime_facts_reach_presentation(&self) -> bool {
        self.presentation_templates(true)
            .into_iter()
            .any(template_reads_runtime_facts)
    }

    #[must_use]
    pub fn window_labels_follow_the_clock(&self) -> bool {
        self.presentation_templates(false)
            .into_iter()
            .any(|template| {
                template.contains('#')
                    && CLOCK_MARKERS.iter().any(|marker| template.contains(marker))
                    || template.contains('%')
            })
    }

    fn presentation_templates(&self, status: bool) -> Vec<&str> {
        const LABEL_SCALARS: [&str; 5] = [
            "pane-border-format",
            "pane-border-style",
            "pane-active-border-style",
            "window-pane-status-format",
            "window-pane-current-status-format",
        ];
        const STATUS_SCALARS: [&str; 3] =
            ["window-style", "window-active-style", "set-titles-string"];
        let names = || {
            LABEL_SCALARS
                .iter()
                .chain(status.then_some(&STATUS_SCALARS).into_iter().flatten())
        };
        let window_status = &self.window_status;
        let mut templates = vec![
            window_status.format.as_str(),
            window_status.current_format.as_str(),
            window_status.separator.as_str(),
            window_status.style.as_str(),
            window_status.current_style.as_str(),
            window_status.last_style.as_str(),
            window_status.bell_style.as_str(),
            window_status.activity_style.as_str(),
        ];
        templates.extend(
            self.window_status_options
                .values()
                .flat_map(BTreeMap::values)
                .map(String::as_str),
        );
        templates.extend(
            names().filter_map(|name| tmux_stored_scalar(name).map(|metadata| metadata.default)),
        );
        let scalar_tables = std::iter::once(&self.stored_scalars.global_session)
            .chain(self.stored_scalars.sessions.values())
            .chain(std::iter::once(&self.stored_scalars.global_window))
            .chain(self.stored_scalars.windows.values())
            .chain(self.stored_scalars.panes.values());
        for table in scalar_tables {
            templates.extend(
                names()
                    .filter_map(|name| table.get(name))
                    .map(String::as_str),
            );
        }
        if !status {
            return templates;
        }
        let formats = &self.status;
        templates.extend([
            formats.left.as_str(),
            formats.right.as_str(),
            formats.style.as_str(),
            formats.background.as_str(),
            formats.foreground.as_str(),
            formats.left_style.as_str(),
            formats.right_style.as_str(),
        ]);
        templates.extend(
            self.session_status_options
                .values()
                .flat_map(BTreeMap::values)
                .map(String::as_str),
        );
        if !self.explicit_status_options.is_empty()
            || !self.session_explicit_status_options.is_empty()
        {
            let default = default_array("status-format");
            for table in std::iter::once(&self.stored_arrays.global_session)
                .chain(self.stored_arrays.sessions.values())
            {
                if let Some(array) = table.get("status-format")
                    && *array != default
                {
                    templates.extend(array.values().map(String::as_str));
                }
            }
        }
        templates
    }

    pub fn mark_pane_dead(
        &mut self,
        pane: PaneId,
        status: Option<u32>,
        signal: Option<&str>,
    ) -> Result<bool, ServerError> {
        let mut hooks = CommandHooks::new(self.format_now());
        self.mark_pane_dead_with_hooks(pane, status, signal, &mut hooks)
    }

    pub fn mark_pane_dead_with_hooks(
        &mut self,
        pane: PaneId,
        status: Option<u32>,
        signal: Option<&str>,
        hooks: &mut impl StatusHooks,
    ) -> Result<bool, ServerError> {
        if self.state.pane(pane).is_none() {
            return Err(ServerError::MissingTarget(pane.to_string()));
        }
        let dead_signal = signal.map_or_else(String::new, tmux_signal_name);
        let facts = self.pane_runtime_facts.entry(pane).or_default();
        let facts_changed = facts.dead_signal != dead_signal;
        facts.dead_signal = dead_signal;
        let state_changed = self.state.mark_pane_dead(pane, status)?;
        if state_changed {
            let format_now = self.format_now();
            self.state
                .pane_mut(pane)
                .expect("dead pane exists")
                .dead_time = (format_now != 0).then_some(format_now);
        }
        if facts_changed && !state_changed {
            self.state.bump_generation();
        }
        let name_changed = self.refresh_automatic_window_name_for_pane(pane, hooks);
        Ok(facts_changed || state_changed || name_changed)
    }

    #[must_use]
    pub fn pane_runtime_facts(&self, pane: PaneId) -> Option<&PaneRuntimeFacts> {
        self.pane_runtime_facts.get(&pane)
    }

    pub fn set_agent_chat(&mut self, pane: PaneId, facts: Option<AgentChatFacts>) -> bool {
        let before = match facts {
            Some(facts) if self.state.pane(pane).is_some() => self.agent_chats.insert(pane, facts),
            Some(_) => return false,
            None => self.agent_chats.remove(&pane),
        };
        before.as_ref() != self.agent_chats.get(&pane)
    }

    #[must_use]
    pub fn agent_chat(&self, pane: PaneId) -> Option<&AgentChatFacts> {
        self.agent_chats.get(&pane)
    }

    pub fn set_agent_parent(&mut self, child: PaneId, parent: PaneId) {
        if self.state.pane(child).is_some() {
            self.agent_parents.insert(child, parent);
        }
    }

    #[must_use]
    pub fn agent_parent(&self, child: PaneId) -> Option<PaneId> {
        self.agent_parents.get(&child).copied()
    }

    #[must_use]
    pub const fn agent_parents(&self) -> &BTreeMap<PaneId, PaneId> {
        &self.agent_parents
    }

    #[must_use]
    pub fn agent_children(&self, parent: PaneId) -> Vec<PaneId> {
        self.agent_parents
            .iter()
            .filter(|(_, candidate)| **candidate == parent)
            .map(|(child, _)| *child)
            .collect()
    }

    pub fn format_host(&self) -> &str {
        &self.format_host
    }

    pub fn format_host_short(&self) -> &str {
        &self.format_host_short
    }

    pub(crate) fn format_socket_path(&self) -> &str {
        &self.format_socket_path
    }

    pub(crate) const fn format_pid(&self) -> u32 {
        self.format_pid
    }

    pub(crate) const fn format_start_time(&self) -> u64 {
        self.format_start_time
    }

    pub(crate) const fn format_now(&self) -> u64 {
        self.state.format_now()
    }

    #[must_use]
    pub fn format_uid(&self) -> &str {
        &self.format_uid
    }

    #[must_use]
    pub fn format_user(&self) -> &str {
        &self.format_user
    }

    #[must_use]
    pub fn pane_index(&self, window: WindowId, pane: PaneId) -> Option<u32> {
        let offset = self
            .state
            .windows
            .get(&window)?
            .pane_order()
            .iter()
            .position(|candidate| *candidate == pane)?;
        self.pane_base_index_for_window(window)
            .checked_add(u32::try_from(offset).ok()?)
    }

    pub fn pane_at_index(&self, window: WindowId, index: u32) -> Option<PaneId> {
        let offset = index.checked_sub(self.pane_base_index_for_window(window))?;
        self.state
            .windows
            .get(&window)?
            .pane_order()
            .get(usize::try_from(offset).ok()?)
            .copied()
    }

    pub fn resolve_session(
        &self,
        target: Option<&str>,
        current: Option<SessionId>,
    ) -> Result<SessionId, ServerError> {
        self.state
            .resolve_session_with_pane_index(target, current, &|window, index| {
                self.pane_at_index(window, index)
            })
    }

    pub fn resolve_window(
        &self,
        target: Option<&str>,
        current_session: Option<SessionId>,
        current_window: Option<WindowId>,
    ) -> Result<WindowId, ServerError> {
        self.state.resolve_window_with_pane_index(
            target,
            current_session,
            current_window,
            &|window, index| self.pane_at_index(window, index),
        )
    }

    pub fn resolve_pane(
        &self,
        target: Option<&str>,
        current_window: Option<WindowId>,
        current_pane: Option<PaneId>,
    ) -> Result<PaneId, ServerError> {
        self.state.resolve_pane_with_index(
            target,
            current_window,
            current_pane,
            &|window, index| self.pane_at_index(window, index),
        )
    }

    fn refresh_automatic_window_name_for_pane(
        &mut self,
        pane: PaneId,
        hooks: &mut impl StatusHooks,
    ) -> bool {
        let Some(window) = self.state.window_for_pane(pane) else {
            return false;
        };
        let Some(window_state) = self.state.windows.get(&window) else {
            return false;
        };
        if window_state.active_pane != pane
            || !self
                .state
                .window_automatic_rename(window)
                .unwrap_or_default()
        {
            return false;
        }
        let session = window_state.session;
        let pane_state = &window_state.panes[&pane];
        let pane_dead = pane_state.dead;
        let format = self.automatic_rename_format_for_window(window).to_owned();
        let agent_title = (format == DEFAULT_AUTOMATIC_RENAME_FORMAT
            && matches!(pane_state.kind, PaneKind::Agent(_)))
        .then(|| RawText::from(pane_state.title.clone()));
        if agent_title.is_none()
            && !pane_dead
            && !self.pane_runtime_facts.contains_key(&pane)
            && format == DEFAULT_AUTOMATIC_RENAME_FORMAT
        {
            return false;
        }
        let mut hooks = CommandItemHooks {
            inner: hooks,
            command: None,
        };
        let name = agent_title.unwrap_or_else(|| {
            expand_format_with_hooks(
                &format,
                self,
                FormatContext {
                    session: Some(session),
                    window: Some(window),
                    pane: None,
                    active_session: Some(session),
                    format_client: FormatClient::NoClient,
                    format_type: FormatType::Window,
                },
                &mut hooks,
            )
        });
        let changed = self.state.windows[&window].name != name;
        if changed {
            self.state
                .rename_window(window, name)
                .expect("automatic rename window exists");
        }
        changed
    }

    pub fn update_agent_title(&mut self, pane: PaneId, title: Option<&str>) -> bool {
        if !self
            .state
            .update_agent_title(pane, title)
            .unwrap_or_default()
        {
            return false;
        }
        let mut hooks = CommandHooks::new(self.format_now());
        self.refresh_automatic_window_name_for_pane(pane, &mut hooks);
        true
    }

    pub fn environment_for_session(
        &self,
        session: SessionId,
    ) -> Result<Vec<(RawText, Option<RawText>)>, ServerError> {
        if !self.state.sessions.contains_key(&session) {
            return Err(ServerError::MissingTarget(session.to_string()));
        }
        Ok(self.job_environment(Some(session)))
    }

    pub fn update_session_environment_from_client(
        &mut self,
        session: SessionId,
        client_environment: &BTreeMap<RawText, RawText>,
    ) -> Result<(), ServerError> {
        if !self.state.sessions.contains_key(&session) {
            return Err(ServerError::MissingTarget(session.to_string()));
        }
        let patterns = self.update_environment_names_for_session(Some(session));
        let retained = self.session_environments.entry(session).or_default();
        apply_client_environment_update(&mut retained.inner.lock(), &patterns, client_environment);
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
        Ok(())
    }

    pub fn retain_session_job_environment(
        &mut self,
        session: SessionId,
    ) -> Option<RetainedJobEnvironment> {
        self.state.sessions.contains_key(&session).then(|| {
            self.session_environments
                .entry(session)
                .or_default()
                .clone()
        })
    }

    pub fn job_environment_with_retained_session(
        &self,
        retained: Option<&RetainedJobEnvironment>,
    ) -> Vec<(RawText, Option<RawText>)> {
        let mut environment = BTreeMap::new();
        extend_job_environment(&mut environment, &self.global_environment);
        if let Some(retained) = retained {
            extend_job_environment(&mut environment, &retained.inner.lock());
        }
        environment.into_iter().collect()
    }

    pub fn job_environment(&self, session: Option<SessionId>) -> Vec<(RawText, Option<RawText>)> {
        let retained = session.and_then(|session| self.session_environments.get(&session));
        self.job_environment_with_retained_session(retained)
    }

    pub fn execute(
        &mut self,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let mut hooks = CommandHooks::new(self.format_now());
        self.execute_with_format_hooks(context, command, &mut hooks)
    }

    pub fn execute_prepared(
        &mut self,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let mut hooks = CommandHooks::new(self.format_now());
        self.execute_without_alias_expansion(context, command, &mut hooks, &mut |_| true)
    }

    pub fn execute_with_format_hooks(
        &mut self,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        self.execute_with_shell_validator(context, command, hooks, &mut |_| true)
    }

    #[must_use]
    pub fn resolve_command_alias(&self, command: &CommandInvocation) -> CommandAliasResolution {
        if command.name.contains('=') {
            return CommandAliasResolution::Miss;
        }
        let Some(alias) = self
            .array_option(TmuxOptionTarget::Server, "command-alias")
            .and_then(|aliases| {
                aliases
                    .values()
                    .find_map(|entry| entry.strip_prefix(command.name.as_str())?.strip_prefix('='))
            })
        else {
            return CommandAliasResolution::Miss;
        };
        let parsed = self.parse_config("<command-alias>", alias);
        if !parsed.diagnostics.is_empty() {
            return CommandAliasResolution::MatchedUnsupported(CommandAliasBodyError::Unparsable);
        }
        let mut commands = parsed.commands;
        if let Some(last) = commands.last_mut() {
            last.append_args(command.clone());
        }
        for expanded in &mut commands {
            expanded.source.clone_from(&command.source);
            if let Some(stdin) = command.stdin() {
                expanded.set_stdin(stdin.clone());
            } else if command.stdin_was_spent() {
                expanded.set_stdin_spent();
            }
        }
        if commands.len() == 1 {
            return CommandAliasResolution::Expanded(commands.remove(0));
        }
        let body = format_callback_commands_round_trip(&commands);
        let mut expanded =
            CommandInvocation::new(COMMAND_ALIAS_GROUP_NAME, [format!("{{ {body} }}")])
                .with_command_blocks([0])
                .into_expanded_alias_group();
        expanded.source.clone_from(&command.source);
        if let Some(stdin) = command.stdin() {
            expanded.set_stdin(stdin.clone());
        } else if command.stdin_was_spent() {
            expanded.set_stdin_spent();
        }
        CommandAliasResolution::Expanded(expanded)
    }

    #[must_use]
    pub fn is_command_alias_group(command: &CommandInvocation) -> bool {
        command_alias_group_body(command).is_some()
    }

    pub fn command_alias_group_commands(
        command: &CommandInvocation,
    ) -> Result<Option<Vec<CommandInvocation>>, ServerError> {
        parse_command_alias_group(command)
    }

    #[must_use]
    pub fn command_alias_group_body(command: &CommandInvocation) -> Option<&str> {
        command_alias_group_body(command)
    }

    pub fn execute_with_shell_validator(
        &mut self,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        hooks: &mut impl StatusHooks,
        default_shell_is_valid: &mut impl FnMut(&str) -> bool,
    ) -> Result<Execution, ServerError> {
        let (mut command, alias_matched) = match self.resolve_command_alias(command) {
            CommandAliasResolution::Miss => (command.clone(), false),
            CommandAliasResolution::Expanded(command) => (command, true),
            CommandAliasResolution::MatchedUnsupported(_) => {
                return Err(ServerError::CommandParse(format!(
                    "unknown command: {}",
                    command.name
                )));
            }
        };
        if (0..command.args.len()).any(|index| command.argument_is_command_block(index)) {
            let owner = canonical_command(&command.name).to_owned();
            self.prepare_expanded_callback_command(&mut command, &owner, alias_matched)?;
        }
        self.execute_without_alias_expansion(context, &command, hooks, default_shell_is_valid)
    }

    pub fn execute_without_alias_expansion(
        &mut self,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        hooks: &mut impl StatusHooks,
        default_shell_is_valid: &mut impl FnMut(&str) -> bool,
    ) -> Result<Execution, ServerError> {
        self.execute_without_alias_expansion_inner(context, command, hooks, default_shell_is_valid)
            .map_err(
                |error| match catalog_command_spec(canonical_command(&command.name)) {
                    Some(spec) => spec.classify_usage_error(error),
                    None => error,
                },
            )
    }

    fn execute_without_alias_expansion_inner(
        &mut self,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        hooks: &mut impl StatusHooks,
        default_shell_is_valid: &mut impl FnMut(&str) -> bool,
    ) -> Result<Execution, ServerError> {
        if let Some(commands) = parse_command_alias_group(command)? {
            validate_static_command_chain(&commands)?;
            let mut combined = Execution::default();
            let mut stream = command.stdin().cloned();
            let mut spent = command.stdin_was_spent();
            for mut command in commands {
                if let Some(stream) = &stream {
                    command.set_stdin(stream.clone());
                } else if spent {
                    command.set_stdin_spent();
                }
                let execution = self.execute_without_alias_expansion(
                    context,
                    &command,
                    hooks,
                    default_shell_is_valid,
                )?;
                if execution.effects.iter().any(|effect| {
                    matches!(
                        effect,
                        MuxEffect::SourceFile {
                            stdin: Some(SourceStream::Bytes(_)),
                            ..
                        } | MuxEffect::PaneStreamInput { .. }
                    )
                }) {
                    stream = None;
                    spent = true;
                }
                if !execution.output.is_empty() {
                    if !combined.output.is_empty() && !combined.output.ends_with('\n') {
                        combined.output.push_bytes(b"\n");
                    }
                    combined.output.push_bytes(execution.output.as_bytes());
                }
                combined.effects.extend(execution.effects);
            }
            return Ok(combined);
        }
        let mouse_resolved = context
            .invoking_mouse()
            .and_then(MouseEventTarget::target_spelling)
            .and_then(|target| resolve_mouse_targets(command, &target));
        let command = mouse_resolved.as_ref().unwrap_or(command);
        let generation = self.state.generation();
        let name = canonical_command(&command.name);
        let mut item_hooks = CommandItemHooks {
            inner: hooks,
            command: Some(name),
        };
        let hooks = &mut item_hooks;
        let mut execution = match name {
            "new-session" => self.new_session(context, &command.args, hooks)?,
            "list-sessions" => self.list_sessions(context, &command.args, hooks)?,
            "rename-session" => self.rename_session(context, &command.args, hooks)?,
            "kill-session" => self.kill_session(context, &command.args, hooks)?,
            "attach-session" => self.attach_session(context, &command.args, hooks)?,
            "has-session" => self.has_session(context, &command.args)?,
            "suspend-client" => self.suspend_client(&command.args)?,
            "detach-client" => self.detach_client(context, &command.args)?,
            "list-clients" | "refresh-client" | "show-messages" => {
                parse_command_options(name, &command.args)?;
                return Err(ServerError::UnsupportedCommand(name.to_owned()));
            }
            "new-window" => self.new_window(context, &command.args, hooks)?,
            "list-windows" => self.list_windows(context, &command.args, hooks)?,
            "rename-window" => self.rename_window(context, &command.args, hooks)?,
            "select-window" => self.select_window(context, &command.args)?,
            "next-window" => self.step_window(context, &command.args, 1)?,
            "previous-window" => self.step_window(context, &command.args, -1)?,
            "last-window" => self.last_window(context, &command.args)?,
            "kill-window" => self.kill_window(context, &command.args, hooks)?,
            "move-window" => self.move_window(context, &command.args)?,
            "swap-window" => self.swap_window(context, &command.args)?,
            "find-window" => self.find_window(context, &command.args)?,
            "split-window" => self.split_window(context, &command.args, command.stdin(), hooks)?,
            "new-pane" => self.new_pane(context, &command.args, command.stdin(), hooks)?,
            "select-pane-kind" => self.select_pane_kind(context, &command.args, hooks)?,
            "break-pane" => self.break_pane(context, &command.args, hooks)?,
            "join-pane" | "move-pane" => self.join_pane(context, &command.args, name, hooks)?,
            "set-browser-url" => self.set_browser_url(context, &command.args)?,
            "set-browser-tabs" => self.set_browser_tabs(context, &command.args)?,
            "set-browser-profile" => self.set_browser_profile(context, &command.args)?,
            "set-editor-path" => self.set_editor_path(context, &command.args)?,
            "set-simulator-device" => self.set_simulator_device(context, &command.args)?,
            "view-pane" => self.view_pane(context, &command.args, hooks)?,
            "select-pane" => self.select_pane(context, &command.args, hooks)?,
            "last-pane" => self.last_pane(context, &command.args, hooks)?,
            "swap-pane" => self.swap_pane(context, &command.args)?,
            "list-panes" => self.list_panes(context, &command.args, hooks)?,
            "resize-pane" => self.resize_pane(context, &command.args)?,
            "resize-window" => self.resize_window(context, &command.args)?,
            "select-layout" | "next-layout" | "previous-layout" => {
                self.select_layout(context, &command.args, name)?
            }
            "rotate-window" => self.rotate_window(context, &command.args)?,
            "kill-pane" => self.kill_pane(context, &command.args, hooks)?,
            "respawn-pane" => self.respawn_pane(context, &command.args, hooks)?,
            "respawn-window" => self.respawn_window(context, &command.args, hooks)?,
            "send-keys" => self.send_keys(context, &command.args, hooks)?,
            "send-prefix" => self.send_prefix(context, &command.args)?,
            "copy-mode" => self.copy_mode(context, &command.args)?,
            "customize-mode" => self.customize_mode(context, &command.args)?,
            "clock-mode" => self.clock_mode(context, &command.args)?,
            "switch-mode" => self.switch_mode(context, command)?,
            "copy-mode-search-prompt" => self.copy_mode_search_prompt(context, &command.args)?,
            "command-prompt" => self.command_prompt(context, command, hooks)?,
            "focus-sidebar" => self.focus_sidebar(context, &command.args)?,
            "choose-path" => self.choose_path(context, &command.args, hooks)?,
            "choose-tree" => self.choose_tree(context, command)?,
            "choose-client" => self.choose_client(context, command)?,
            "choose-buffer" => self.choose_buffer(context, command)?,
            "display-message" => {
                self.display_message(context, &command.args, command.stdin(), hooks)?
            }
            "display-panes" => self.display_panes(context, command)?,
            "clear-history" => self.clear_history(context, &command.args)?,
            "bind-key" => self.bind_key(command)?,
            "unbind-key" => self.unbind_key(&command.args)?,
            "list-keys" => self.list_keys(context, &command.args, hooks)?,
            "list-commands" => self.list_commands(context, &command.args, hooks)?,
            "set-hook" => self.set_hook(context, command, hooks, default_shell_is_valid)?,
            "show-hooks" => self.show_hooks(context, &command.args, hooks)?,
            "set-option" => {
                self.set_option(context, command, false, hooks, default_shell_is_valid)?
            }
            "set-window-option" => {
                self.set_option(context, command, true, hooks, default_shell_is_valid)?
            }
            "show-options" => self.show_options(
                context,
                &command.args,
                false,
                ShowOptionArgument::Expand,
                hooks,
            )?,
            "show-window-options" => self.show_options(
                context,
                &command.args,
                true,
                ShowOptionArgument::Expand,
                hooks,
            )?,
            "set-environment" => self.set_environment(context, &command.args, hooks)?,
            "show-environment" => self.show_environment(context, &command.args)?,
            "source-file" => self.source_file(
                context,
                &command.args,
                command.stdin(),
                command.stdin_was_spent(),
                hooks,
            )?,
            "reload-config" => {
                parse_command_options("reload-config", &command.args)?;
                if command.args.is_empty() {
                    Execution::effect(MuxEffect::ReloadConfig)
                } else {
                    return Err(ServerError::CommandParse(
                        "reload-config does not take arguments".to_owned(),
                    ));
                }
            }
            "import-tmux-config" => {
                let (_, positional) = parse_command_options("import-tmux-config", &command.args)?;
                if positional.len() > 1 {
                    return Err(ServerError::CommandParse(
                        "usage: import-tmux-config [path]".to_owned(),
                    ));
                }
                Execution::effect(MuxEffect::ImportTmuxConfig(
                    positional.first().map(ToString::to_string),
                ))
            }
            "start-server" => {
                let (_, positional) = parse_command_options("start-server", &command.args)?;
                reject_positionals("start-server", &positional)?;
                Execution::default()
            }
            "server-access" => self.server_access(context, &command.args, hooks)?,
            "kill-server" => {
                parse_command_options("kill-server", &command.args)?;
                Execution::effect(MuxEffect::KillServer)
            }
            _ => {
                return Err(match resolve_command(&command.name) {
                    CommandResolution::Unimplemented(name) => {
                        ServerError::UnsupportedCommand(name.to_owned())
                    }
                    CommandResolution::Ambiguous(message) => ServerError::CommandParse(message),
                    _ => ServerError::CommandParse(format!("unknown command: {}", command.name)),
                });
            }
        };

        self.retain_pane_start_commands(&execution.effects);
        let created_panes = execution
            .effects
            .iter()
            .filter_map(|effect| match effect {
                MuxEffect::PaneCreated { pane, .. } | MuxEffect::PaneMaterialized { pane, .. } => {
                    Some(*pane)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for pane in created_panes {
            self.refresh_automatic_window_name_for_pane(pane, hooks);
        }
        if self.state.generation() != generation {
            execution.effects.push(MuxEffect::SnapshotChanged);
        }
        if self.state.removals() != self.swept_removals {
            self.swept_removals = self.state.removals();
            self.retain_live_entries();
        } else if cfg!(debug_assertions) {
            assert!(
                !self.retain_live_entries(),
                "{name} left an entry for a session, window or pane removed without a count"
            );
        }
        self.repair_context(context);
        Ok(execution)
    }

    fn retain_live_entries(&mut self) -> bool {
        let mut removed = false;
        let sessions = &self.state.sessions;
        let windows = &self.state.windows;
        let panes = windows
            .values()
            .flat_map(|window| window.panes.keys().copied())
            .collect::<HashSet<_>>();
        macro_rules! retain_live {
            ($live:expr; $($map:ident),+ $(,)?) => {
                $(
                    let before = self.$map.len();
                    self.$map.retain(|key, _| $live(key));
                    removed |= self.$map.len() != before;
                )+
            };
        }
        retain_live!(
            |session| sessions.contains_key(session);
            session_history_limits,
            session_base_indices,
            session_renumber_windows,
            session_word_separators,
            session_mouse,
            session_display_time_ms,
            session_initial_repeat_time_ms,
            session_repeat_time_ms,
            session_options,
            session_default_commands,
            session_default_shells,
            session_lock_commands,
            session_lock_after_times,
            session_user_options,
            session_hooks,
            session_environments,
        );
        retain_live!(
            |window| windows.contains_key(window);
            window_user_options,
            window_options,
            window_pane_options,
            window_hooks,
            window_mode_keys,
            window_automatic_rename_formats,
            window_remain_on_exit,
            window_popup_styles,
            window_popup_border_styles,
            window_popup_border_lines,
            window_menu_styles,
            window_menu_selected_styles,
            window_menu_border_styles,
            window_menu_border_lines,
        );
        let before = self.state.window_pane_base_indices.len();
        self.state
            .window_pane_base_indices
            .retain(|window, _| windows.contains_key(window));
        removed |= self.state.window_pane_base_indices.len() != before;
        retain_live!(
            |pane| panes.contains(pane);
            pane_user_options,
            pane_options,
            pane_hooks,
            pane_start_commands,
            pane_shells,
            pane_runtime_facts,
            agent_chats,
            agent_parents,
            pane_remain_on_exit,
        );
        removed
    }

    fn pane_target_description(&self, pane: PaneId) -> Result<String, ServerError> {
        let window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window_state = &self.state.windows[&window];
        let session = &self.state.sessions[&window_state.session];
        let pane_index = self
            .pane_index(window, pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        Ok(format!(
            "{}:{}.{}",
            session.name, window_state.index, pane_index
        ))
    }

    fn window_target_description(&self, window: WindowId) -> Result<String, ServerError> {
        let window_state = self
            .state
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let session = &self.state.sessions[&window_state.session];
        Ok(format!("{}:{}", session.name, window_state.index))
    }

    fn send_keys(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("send-keys", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let target_client = options.value("-c").map(str::to_owned);
        let repeat = if let Some(value) = options.value("-N") {
            let target = ExecutionContext::for_pane(&self.state, pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            let expanded = self.expand_pane_format(
                value,
                &target,
                context.session,
                context.target_format_client(),
                hooks,
            );
            parse_repeat_count(&expanded)?
        } else {
            1
        };
        if options.value("-N").is_some() && positional.is_empty() && !options.has("-X") {
            let mut execution = Execution::effect(MuxEffect::CopyModeRepeat {
                pane,
                count: repeat,
                target_client: target_client.clone(),
                require_mode: false,
            });
            if options.has("-R") {
                execution.effects.push(MuxEffect::ResetPane { pane });
            }
            return Ok(execution);
        }
        if options.has("-X") {
            let Some(command) = positional.first() else {
                return Ok(Execution::effect(MuxEffect::CopyModeRepeat {
                    pane,
                    count: if options.value("-N").is_some() {
                        repeat
                    } else {
                        0
                    },
                    target_client,
                    require_mode: true,
                }));
            };
            let mut arguments = positional[1..].to_vec();
            if options.has("-F")
                && SEND_KEYS_FORMAT_EXPANDED_ACTIONS.contains(&command.as_str())
                && let Some(index) = send_keys_search_string_index(&arguments)
                && let Some(argument) = arguments.get_mut(index)
            {
                let target = ExecutionContext::for_pane(&self.state, pane)
                    .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                *argument = self
                    .expand_pane_format(
                        argument,
                        &target,
                        context.session,
                        context.target_format_client(),
                        hooks,
                    )
                    .into();
                if argument.is_empty() {
                    return Ok(Execution::effect(MuxEffect::CopyModeRepeat {
                        pane,
                        count: 1,
                        target_client,
                        require_mode: true,
                    }));
                }
            }
            let Some(action) =
                copy_mode_action(command, &arguments, self.set_clipboard, &self.copy_command)?
            else {
                return Ok(Execution::effect(MuxEffect::CopyModeRepeat {
                    pane,
                    count: 1,
                    target_client,
                    require_mode: true,
                }));
            };
            let action = match action {
                CopyModeAction::LineNumbersOn { .. }
                | CopyModeAction::LineNumbersToggle { .. }
                | CopyModeAction::GotoLine { .. } => {
                    let target = ExecutionContext::for_pane(&self.state, pane)
                        .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                    let option = self.expand_pane_format(
                        "#{copy-mode-line-numbers}",
                        &target,
                        context.session,
                        context.target_format_client(),
                        hooks,
                    );
                    let option_off = option == "off";
                    match action {
                        CopyModeAction::LineNumbersOn { .. } => {
                            CopyModeAction::LineNumbersOn { option_off }
                        }
                        CopyModeAction::GotoLine { line, .. } => CopyModeAction::GotoLine {
                            line,
                            option_absolute: matches!(
                                option.as_str(),
                                "absolute" | "relative" | "hybrid"
                            ),
                        },
                        _ => CopyModeAction::LineNumbersToggle { option_off },
                    }
                }
                action => action,
            };
            let action = match action.count_policy() {
                CopyModeCountPolicy::Once => TerminalViewAction::CopyMode(action),
                _ if repeat == 1 => TerminalViewAction::CopyMode(action),
                _ => TerminalViewAction::CopyModeCounted {
                    action,
                    count: repeat,
                },
            };
            let mut effects = Self::mouse_cursor_move(context, pane);
            effects.push(MuxEffect::TerminalView {
                pane,
                action,
                target_client,
                require_mode: true,
            });
            return Ok(Execution {
                output: RawText::default(),
                effects,
            });
        }
        if options.has("-M") {
            let Some(mouse) = context.invoking_mouse() else {
                return Err(ServerError::InvalidCommand("no mouse target".to_owned()));
            };
            let Some(target) = mouse.pane else {
                return Err(ServerError::InvalidCommand("no mouse target".to_owned()));
            };
            let Some(action) = mouse.view_action.clone() else {
                return Ok(Execution::default());
            };
            return Ok(Execution::effect(MuxEffect::TerminalView {
                pane: target,
                action,
                target_client,
                require_mode: false,
            }));
        }
        let reset = options.has("-R");
        if reset && positional.is_empty() {
            return Ok(Execution::effect(MuxEffect::ResetPane { pane }));
        }
        if options.has("-K") {
            let mut execution = Execution::default();
            if reset {
                execution.effects.push(MuxEffect::ResetPane { pane });
            }
            let keys = if positional.is_empty() {
                context
                    .invoking_key()
                    .map(|key| vec![KeyToken::Named(key.to_owned())])
                    .unwrap_or_default()
            } else if options.has("-H") {
                positional
                    .iter()
                    .filter_map(|value| hex_key_token(value))
                    .collect()
            } else if options.has("-l") {
                vec![KeyToken::Literal(positional.concat())]
            } else {
                positional.iter().map(|value| key_token(value)).collect()
            };
            if !keys.is_empty() {
                execution.effects.push(MuxEffect::SendClientKeys {
                    target_client,
                    keys,
                    repeat,
                });
            }
            return Ok(execution);
        }
        let keys = if options.has("-H") {
            positional
                .iter()
                .filter_map(|value| hex_key_token(value))
                .collect()
        } else if options.has("-l") {
            vec![KeyToken::Literal(positional.concat())]
        } else {
            positional
                .iter()
                .filter_map(|value| {
                    if value == "BSpace" {
                        return match self.server_options.backspace.as_str() {
                            "\x7f" | "0x7f" => Some(KeyToken::Literal("\x7f".to_owned())),
                            "C-\x7f" => None,
                            _ => Some(key_token(value)),
                        };
                    }
                    Some(key_token(value))
                })
                .collect()
        };
        if keys.is_empty()
            && (options.has("-H")
                || (positional.iter().any(|value| value == "BSpace")
                    && self.server_options.backspace == "C-\x7f"))
        {
            return Ok(if reset {
                Execution::effect(MuxEffect::ResetPane { pane })
            } else {
                Execution::default()
            });
        }
        let mut execution = Execution::default();
        if reset {
            execution.effects.push(MuxEffect::ResetPane { pane });
        }
        execution
            .effects
            .push(MuxEffect::SendKeys { pane, keys, repeat });
        Ok(execution)
    }

    fn mouse_cursor_move(context: &ExecutionContext, pane: PaneId) -> Vec<MuxEffect> {
        let Some(mouse) = context.invoking_mouse() else {
            return Vec::new();
        };
        if mouse.pane != Some(pane) {
            return Vec::new();
        }
        match mouse.view_action.as_ref() {
            Some(TerminalViewAction::Mouse(input)) => vec![MuxEffect::TerminalView {
                pane,
                action: TerminalViewAction::CopyMode(CopyModeAction::MouseCursor(input.cell)),
                target_client: None,
                require_mode: true,
            }],
            _ => Vec::new(),
        }
    }

    fn send_prefix(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, _) = parse_command_options("send-prefix", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let key = if options.has("-2") {
            let Some(prefix2) = self.keys.prefix2() else {
                return Ok(Execution::default());
            };
            prefix2
        } else {
            self.keys.prefix()
        };
        let key = match key {
            "\x7f" | "0x7f" => KeyToken::Literal("\x7f".to_owned()),
            "C-\x7f" => return Ok(Execution::default()),
            key => KeyToken::Named(tmux_key_display(key)),
        };
        Ok(Execution::effect(MuxEffect::SendKeys {
            pane,
            keys: vec![key],
            repeat: 1,
        }))
    }

    fn copy_mode(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("copy-mode", args)?;
        reject_positionals("copy-mode", &positional)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        if options.has("-q") {
            return Ok(Execution {
                output: RawText::default(),
                effects: vec![
                    MuxEffect::PaneModeChanged { pane, mode: None },
                    MuxEffect::TerminalView {
                        pane,
                        action: TerminalViewAction::CopyMode(CopyModeAction::Cancel),
                        target_client: None,
                        require_mode: false,
                    },
                ],
            });
        }
        if options.has("-M") {
            let Some(mouse) = context.invoking_mouse() else {
                return Ok(Execution::default());
            };
            let Some(target) = mouse.pane else {
                return Ok(Execution::default());
            };
            let mut effects = vec![MuxEffect::TerminalView {
                pane: target,
                action: TerminalViewAction::EnterCopyModeWith {
                    scroll_exit: false,
                    hide_position: true,
                },
                target_client: None,
                require_mode: false,
            }];
            effects.extend(
                [mouse.press_action.clone(), mouse.view_action.clone()]
                    .into_iter()
                    .flatten()
                    .map(|action| MuxEffect::TerminalView {
                        pane: target,
                        action,
                        target_client: None,
                        require_mode: true,
                    }),
            );
            return Ok(Execution {
                output: RawText::default(),
                effects,
            });
        }
        let mut effects = Vec::new();
        if let Some(source) = options.value("-s") {
            let source = self.resolve_pane(Some(source), context.window, context.pane)?;
            if source != pane {
                effects.push(MuxEffect::ArmCopyModeSource { pane, source });
            }
        }
        if options.has("-k") {
            effects.push(MuxEffect::ArmCopyModeKill { pane });
        }
        let hide_position = options.has("-H") || context.invoking_mouse().is_some();
        effects.push(MuxEffect::TerminalView {
            pane,
            action: if hide_position {
                TerminalViewAction::EnterCopyModeWith {
                    scroll_exit: options.has("-e"),
                    hide_position: true,
                }
            } else if options.has("-e") {
                TerminalViewAction::EnterCopyModeScrollExit
            } else {
                TerminalViewAction::EnterCopyMode
            },
            target_client: None,
            require_mode: false,
        });
        if options.has("-u") {
            effects.push(MuxEffect::TerminalView {
                pane,
                action: TerminalViewAction::CopyMode(CopyModeAction::PageUp),
                target_client: None,
                require_mode: false,
            });
        }
        if options.has("-d") {
            effects.push(MuxEffect::TerminalView {
                pane,
                action: TerminalViewAction::CopyMode(if options.has("-e") {
                    CopyModeAction::PageDownScrollExit
                } else {
                    CopyModeAction::PageDown
                }),
                target_client: None,
                require_mode: false,
            });
        }
        Ok(Execution {
            output: RawText::default(),
            effects,
        })
    }

    fn copy_mode_search_prompt(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, _) = parse_command_options("copy-mode-search-prompt", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::TerminalUi {
            pane,
            command: TerminalUiCommand::BeginSearch {
                direction: if options.has("-b") {
                    SearchDirection::Backward
                } else {
                    SearchDirection::Forward
                },
            },
        }))
    }

    fn command_prompt(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("command-prompt").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("command-prompt", args)?;
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "command-prompt accepts at most one template".to_owned(),
            ));
        }
        let prompt_type = match options.value("-T") {
            None | Some("command") => CommandPromptType::Command,
            Some("search") => CommandPromptType::Search,
            Some(other) => {
                return Err(ServerError::InvalidCommand(format!(
                    "unknown type: {other}"
                )));
            }
        };
        let mode = command_prompt_mode(&options);
        let template = positional
            .first()
            .map(|template| {
                if invocation.argument_is_command_block(positional_start) {
                    let body = crate::parser::command_block_body(template).unwrap_or(template);
                    self.prepare_frozen_callback_commands(body, true, "command-prompt")
                        .map(CommandPromptTemplate::Commands)
                } else {
                    Ok(CommandPromptTemplate::String(template.to_string()))
                }
            })
            .transpose()?;
        let (labels, pad) = match options.value("-p") {
            Some(labels) => (labels.to_owned(), true),
            None => match &template {
                Some(template) => (
                    format!("({})", command_prompt_template_name(template)),
                    true,
                ),
                None => (":".to_owned(), false),
            },
        };
        let inputs = options.value("-I");
        let steps = if options.has("-l") {
            vec![CommandPromptStep {
                label: labels,
                input: self.expand_prompt_input(context, inputs.unwrap_or_default(), hooks),
            }]
        } else {
            let mut inputs = inputs.map(|inputs| inputs.split(','));
            labels
                .split(',')
                .map(|label| {
                    Ok(CommandPromptStep {
                        label: if pad {
                            format!("{label} ")
                        } else {
                            label.to_owned()
                        },
                        input: self.expand_prompt_input(
                            context,
                            inputs.as_mut().and_then(Iterator::next).unwrap_or_default(),
                            hooks,
                        ),
                    })
                })
                .collect::<Result<Vec<_>, ServerError>>()?
        };
        if steps
            .iter()
            .any(|step| step.label.len() > MAX_COMMAND_PROMPT_LABEL_BYTES)
        {
            return Err(ServerError::InvalidCommand(format!(
                "command prompt label exceeds {MAX_COMMAND_PROMPT_LABEL_BYTES} bytes"
            )));
        }
        if steps
            .iter()
            .any(|step| step.input.len() > zz_protocol::MAX_COMMAND_PROMPT_BYTES)
        {
            return Err(ServerError::InvalidCommand(format!(
                "command prompt input exceeds {} bytes",
                zz_protocol::MAX_COMMAND_PROMPT_BYTES
            )));
        }
        if template.as_ref().is_some_and(|template| match template {
            CommandPromptTemplate::String(template) => {
                template.len() > MAX_COMMAND_PROMPT_TEMPLATE_BYTES
            }
            CommandPromptTemplate::Commands(commands) => {
                format_callback_commands(commands).len() > MAX_COMMAND_PROMPT_TEMPLATE_BYTES
            }
        }) {
            return Err(ServerError::InvalidCommand(format!(
                "command prompt template exceeds {MAX_COMMAND_PROMPT_TEMPLATE_BYTES} bytes"
            )));
        }
        let pane = if options.has("-P") {
            match self.resolve_pane(None, context.window, context.pane) {
                Ok(pane) => Some(pane),
                Err(_) => return Ok(Execution::default()),
            }
        } else {
            None
        };
        Ok(Execution::effect(MuxEffect::CommandPrompt {
            steps,
            template,
            source: invocation.source.clone(),
            prompt_type,
            mode,
            no_freeze: options.has("-C"),
            pane,
        }))
    }

    fn expand_prompt_input(
        &self,
        context: &ExecutionContext,
        input: &str,
        hooks: &mut impl StatusHooks,
    ) -> String {
        let format_context = self
            .resolve_pane(None, context.window, context.pane)
            .ok()
            .and_then(|pane| ExecutionContext::for_pane(&self.state, pane))
            .map_or_else(FormatContext::default, |target| FormatContext {
                session: target.session,
                window: target.window,
                pane: target.pane,
                active_session: None,
                format_client: FormatClient::NoClient,
                format_type: FormatType::Pane,
            });
        expand_format_time_with_hooks(input, self, format_context, hooks).into()
    }

    fn clear_history(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, _) = parse_command_options("clear-history", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::TerminalView {
            pane,
            action: TerminalViewAction::ClearHistory,
            target_client: None,
            require_mode: false,
        }))
    }

    fn clock_mode(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("clock-mode", args)?;
        reject_positionals("clock-mode", &positional)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::PaneModeChanged {
            pane,
            mode: Some(PaneModeRequest::Clock),
        }))
    }

    fn switch_mode(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("switch-mode").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("switch-mode", args)?;
        spec.validate_positional_maximum(positional.len())?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::PaneModeChanged {
            pane,
            mode: Some(PaneModeRequest::Switch(Box::new(
                SwitchMode::new(
                    options.has("-w"),
                    options.value("-F").map(str::to_owned),
                    chooser_command_template(invocation, positional_start, &positional),
                    options.has("-k"),
                    options.has("-Z"),
                    self.word_separators_for_pane(pane)?,
                )
                .with_status_keys(self.pane_prompt_keys_are_vi(pane)),
            ))),
        }))
    }

    fn choose_tree(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("choose-tree").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("choose-tree", args)?;
        spec.validate_positional_maximum(positional.len())?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let sort = TmuxSort::parse(
            options.value("-O"),
            options.has("-r"),
            Some(TmuxSortOrder::Index),
        )?;
        Ok(Execution::effect(MuxEffect::ChooseTree {
            pane,
            preview: chooser_preview(&options),
            kind: if options.has("-s") || options.has("-w") {
                ChooseTreeKind::Windows
            } else {
                ChooseTreeKind::Panes
            },
            info_preview: false,
            sessions_only: options.has("-s"),
            filter: options.value("-f").map(str::to_owned),
            format: options.value("-F").map(str::to_owned),
            hide_source: options.has("-h"),
            kill_source: options.has("-k"),
            prompt_accept: options.has("-y"),
            sort,
            key_format: options.value("-K").map(str::to_owned),
            template: chooser_command_template(invocation, positional_start, &positional),
            zoom: options.has("-Z"),
        }))
    }

    fn choose_client(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("choose-client").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("choose-client", args)?;
        spec.validate_positional_maximum(positional.len())?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let sort = TmuxSort::parse(
            options.value("-O"),
            options.has("-r"),
            Some(TmuxSortOrder::Name),
        )?;
        Ok(Execution::effect(MuxEffect::ChooseTree {
            pane,
            preview: chooser_preview(&options),
            kind: ChooseTreeKind::Clients,
            info_preview: options.has("-i"),
            sessions_only: false,
            filter: options.value("-f").map(str::to_owned),
            format: options.value("-F").map(str::to_owned),
            hide_source: options.has("-h"),
            kill_source: options.has("-k"),
            prompt_accept: options.has("-y"),
            sort,
            key_format: options.value("-K").map(str::to_owned),
            template: chooser_command_template(invocation, positional_start, &positional),
            zoom: options.has("-Z"),
        }))
    }

    fn focus_sidebar(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("focus-sidebar", args)?;
        if !positional.is_empty() {
            return Err(ServerError::CommandParse(
                "focus-sidebar does not take positional arguments".to_owned(),
            ));
        }
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        Ok(Execution::effect(MuxEffect::FocusSidebar { pane }))
    }

    fn choose_path(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("choose-path", args)?;
        if !positional.is_empty() {
            return Err(ServerError::CommandParse(
                "choose-path does not take positional arguments".to_owned(),
            ));
        }
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let start_dir = options.value("-c").and_then(|value| {
            let format_context = ExecutionContext::for_pane(&self.state, pane).map_or_else(
                FormatContext::default,
                |origin| FormatContext {
                    session: origin.session,
                    window: origin.window,
                    pane: origin.pane,
                    active_session: origin.session,
                    format_client: context.format_client(),
                    format_type: FormatType::Pane,
                },
            );
            let expanded = expand_format_with_hooks(value, self, format_context, hooks);
            (!expanded.is_empty()).then(|| expanded.to_string())
        });
        Ok(Execution::effect(MuxEffect::ChoosePath { pane, start_dir }))
    }

    fn choose_buffer(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("choose-buffer").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("choose-buffer", args)?;
        spec.validate_positional_maximum(positional.len())?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let sort = TmuxSort::parse(
            options.value("-O"),
            options.has("-r"),
            Some(TmuxSortOrder::Creation),
        )?;
        Ok(Execution::effect(MuxEffect::ChooseBuffer {
            pane,
            preview: chooser_preview(&options),
            filter: options.value("-f").map(str::to_owned),
            format: options.value("-F").map(str::to_owned),
            kill_source: options.has("-k"),
            sort,
            key_format: options.value("-K").map(str::to_owned),
            template: chooser_command_template(invocation, positional_start, &positional),
            zoom: options.has("-Z"),
        }))
    }

    fn display_message(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        stdin: Option<&RawText>,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_display_message_options(args)?;
        let target = self.resolve_display_message_context(context, &options)?;
        let pane = target.as_ref().and_then(|target| target.pane);
        let json = options.has("-j");
        if options.has("-I") && !json {
            let Some(pane) = pane else {
                return Ok(Execution::default());
            };
            return self.pane_stream_input(pane, stdin);
        }
        let format_context = target.map_or_else(FormatContext::default, |target| FormatContext {
            session: target.session,
            window: target.window,
            pane: target.pane,
            active_session: context.session,
            format_client: context.target_format_client(),
            format_type: FormatType::Pane,
        });
        let format = if positional.is_empty() {
            options
                .value("-F")
                .unwrap_or(if json { "" } else { DEFAULT_DISPLAY_MESSAGE })
                .to_owned()
        } else {
            if options.value("-F").is_some() {
                return Err(ServerError::CommandParse(
                    "only one of -F or argument must be given".to_owned(),
                ));
            }
            positional.join(" ")
        };
        let delay = options
            .value("-d")
            .map(|value| parse_strtonum(value, 0, i64::from(u32::MAX), "delay"))
            .transpose()?;
        if options.has("-a") && !json {
            let mut listing = String::new();
            for (name, value) in format_listing(self, format_context, hooks) {
                let _ = writeln!(listing, "{name}={value}");
            }
            return Ok(Execution::output(listing.trim_end_matches('\n').to_owned()));
        }
        let mut trace = Vec::new();
        let text = if options.has("-l") {
            RawText::from(format)
        } else if options.has("-v") {
            let (text, lines) = expand_format_time_traced(&format, self, format_context, hooks);
            trace = lines;
            text
        } else {
            expand_format_time_with_hooks(&format, self, format_context, hooks)
        };
        let text = if json {
            let text = text.to_string();
            crate::layout::json::parse(&text)
                .map(|node| RawText::from(node.to_json()))
                .map_err(ServerError::InvalidCommand)?
        } else {
            text
        };
        if options.has("-p") {
            let mut text = text;
            if text.is_empty() || text.as_bytes().ends_with(b"\n") {
                text.push_bytes(b"\n");
            }
            if trace.is_empty() {
                return Ok(Execution::output(text));
            }
            let mut output = RawText::from(trace.join("\n"));
            output.push_bytes(b"\n");
            output.push_bytes(text.as_bytes());
            return Ok(Execution::output(output));
        }
        if !trace.is_empty() {
            let mut execution = Execution::output(trace.join("\n"));
            execution.effects.push(MuxEffect::DisplayMessage {
                pane,
                target_client: options.value("-c").map(str::to_owned),
                text: text.to_string(),
                duration_ms: delay
                    .map(|delay| u32::try_from(delay).expect("delay was bounded to u32")),
                freeze: !options.has("-C"),
                ignore_keys: options.has("-N"),
            });
            return Ok(execution);
        }
        {
            Ok(Execution::effect(MuxEffect::DisplayMessage {
                pane,
                target_client: options.value("-c").map(str::to_owned),
                text: text.to_string(),
                duration_ms: delay
                    .map(|delay| u32::try_from(delay).expect("delay was bounded to u32")),
                freeze: !options.has("-C"),
                ignore_keys: options.has("-N"),
            }))
        }
    }

    pub fn command_stdin_destination_ready(
        &self,
        context: &ExecutionContext,
        command: &CommandInvocation,
    ) -> Result<bool, ServerError> {
        let name = canonical_command(&command.name);
        if !matches!(name, "display-message" | "split-window") {
            return Ok(true);
        }
        let (options, positional) = parse_command_options(name, &command.args)?;
        if name == "display-message" {
            let target = self.resolve_display_message_context(context, &options)?;
            let Some(pane) = target.and_then(|target| target.pane) else {
                return Ok(false);
            };
            self.pane_stream_input(pane, None)?;
        } else {
            validate_spawn_kind(name, &options, &positional)?;
            self.resolve_pane(options.value("-t"), context.window, context.pane)?;
            pane_spawn_empty(&options, shell_command_positional(&positional).as_deref())?;
            self.split_placement(&options, split_size(&options))?;
        }
        Ok(true)
    }

    fn pane_stream_input(
        &self,
        pane: PaneId,
        stdin: Option<&RawText>,
    ) -> Result<Execution, ServerError> {
        if !self.state.pane(pane).is_some_and(|state| state.empty) {
            return Err(ServerError::InvalidCommand("pane is not empty".to_owned()));
        }
        Ok(stdin.map_or_else(Execution::default, |bytes| {
            Execution::effect(MuxEffect::PaneStreamInput {
                pane,
                bytes: bytes.clone(),
            })
        }))
    }

    pub fn display_message_format_target(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<(Option<ExecutionContext>, Option<String>), ServerError> {
        let (options, _) = parse_command_options("display-message", args)?;
        Ok((
            self.resolve_display_message_context(context, &options)?,
            options.value("-c").map(str::to_owned),
        ))
    }

    fn resolve_display_message_context(
        &self,
        context: &ExecutionContext,
        options: &Options,
    ) -> Result<Option<ExecutionContext>, ServerError> {
        match options.value("-t") {
            Some(target) => match self.resolve_pane(Some(target), context.window, context.pane) {
                Ok(pane) => Ok(ExecutionContext::for_pane(&self.state, pane)),
                Err(
                    ServerError::SessionNotFound(_)
                    | ServerError::WindowNotFound(_)
                    | ServerError::PaneNotFound(_),
                ) => Ok(self.resolve_display_message_partial_context(context, target)),
                Err(ServerError::InvalidCommand(message)) if message == NO_MARKED_TARGET => {
                    Ok(None)
                }
                Err(error) => Err(error),
            },
            None if self.state.sessions.is_empty() => Ok(None),
            None => self
                .resolve_pane(None, context.window, context.pane)
                .map(|pane| ExecutionContext::for_pane(&self.state, pane)),
        }
    }

    fn resolve_display_message_partial_context(
        &self,
        context: &ExecutionContext,
        target: &str,
    ) -> Option<ExecutionContext> {
        let window = if let Some((session_target, window_and_pane)) = target.split_once(':') {
            let session = self
                .resolve_session(
                    (!session_target.is_empty()).then_some(session_target),
                    context.session,
                )
                .ok()?;
            let active_window = self.state.sessions.get(&session)?.active_window;
            let window_target = window_and_pane
                .split_once('.')
                .map_or(window_and_pane, |(window, _)| window);
            if window_target.is_empty() {
                active_window
            } else {
                let target = format!("{session}:{window_target}");
                self.resolve_window(Some(&target), Some(session), context.window)
                    .unwrap_or(active_window)
            }
        } else {
            let (window_target, _) = target.split_once('.')?;
            if is_marked_target(window_target) {
                return None;
            }
            self.resolve_window(
                (!window_target.is_empty()).then_some(window_target),
                context.session,
                context.window,
            )
            .ok()?
        };
        let window_state = self.state.windows.get(&window)?;
        Some(ExecutionContext::new(
            Some(window_state.session),
            Some(window),
            Some(window_state.active_pane),
        ))
    }

    fn display_panes(
        &self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
    ) -> Result<Execution, ServerError> {
        let args = &invocation.args;
        let spec = command_spec("display-panes").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = args.len().saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("display-panes", args)?;
        let template = positional
            .first()
            .map(|template| {
                if invocation.argument_is_command_block(positional_start) {
                    let body = crate::parser::command_block_body(template).unwrap_or(template);
                    self.prepare_frozen_callback_commands(body, true, "display-panes")
                        .map(CommandPromptTemplate::Commands)
                } else {
                    Ok(CommandPromptTemplate::String(template.to_string()))
                }
            })
            .transpose()?;
        let source_window = options
            .value("-s")
            .map(|target| self.resolve_window(Some(target), context.session, context.window))
            .transpose()?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let target_window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let source_window = source_window.unwrap_or(target_window);
        let source_session = if source_window == target_window {
            context
                .session
                .filter(|session| {
                    self.state
                        .sessions
                        .get(session)
                        .is_some_and(|state| state.windows.contains(&target_window))
                })
                .unwrap_or(self.state.windows[&target_window].session)
        } else {
            self.state.windows[&source_window].session
        };
        Ok(Execution::effect(MuxEffect::DisplayPanes {
            pane,
            delay: options.value("-d").map(str::to_owned),
            selectable: !options.has("-N"),
            template,
            source: invocation.source.clone(),
            source_window,
            source_session,
            kill_source: options.has("-k"),
            zoom: !options.has("-Z"),
        }))
    }

    fn set_environment(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("set-environment", args)?;
        if !(1..=2).contains(&positional.len()) {
            return Err(ServerError::CommandParse(
                "set-environment needs a variable and optional value".to_owned(),
            ));
        }
        let name = &positional[0];
        validate_environment_name(name)?;
        let target_session = if options.has("-g") {
            self.resolve_session(options.value("-t"), context.session)
                .ok()
        } else {
            Some(self.resolve_session(options.value("-t"), context.session)?)
        };
        let mut value = positional.get(1).cloned();
        if options.has("-F")
            && let Some(raw) = value.as_deref()
        {
            value = Some(expand_format_with_hooks(
                raw,
                self,
                self.environment_format_context(
                    target_session,
                    context.session,
                    context.target_format_client(),
                ),
                hooks,
            ));
        }
        if (options.has("-r") || options.has("-u")) && value.is_some() {
            let flag = if options.has("-u") { "-u" } else { "-r" };
            return Err(ServerError::InvalidCommand(format!(
                "can't specify a value with {flag}"
            )));
        }
        let value = if options.has("-u") || options.has("-r") {
            None
        } else {
            Some(
                value
                    .ok_or_else(|| ServerError::InvalidCommand("no value specified".to_owned()))?,
            )
        };
        let apply = |environment: &mut Environment| {
            if options.has("-u") {
                environment.remove(name);
            } else {
                let hidden = if options.has("-r") {
                    environment.get(name).is_some_and(|entry| entry.hidden)
                } else {
                    options.has("-h")
                };
                environment.insert(
                    name.clone(),
                    EnvironmentEntry {
                        value: value.clone(),
                        hidden,
                    },
                );
            }
        };
        if options.has("-g") {
            apply(&mut self.global_environment);
        } else {
            let retained = self
                .session_environments
                .entry(target_session.expect("local environment has a session"))
                .or_default();
            apply(&mut retained.inner.lock());
        }
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
        Ok(Execution::default())
    }

    fn show_environment(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("show-environment", args)?;
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "show-environment accepts at most one variable".to_owned(),
            ));
        }
        if options.has("-g") {
            if let Some(target) = options.value("-t") {
                self.resolve_session(Some(target), context.session)?;
            }
            return show_environment_entries(&self.global_environment, &positional, &options);
        }
        let session = self.resolve_session(options.value("-t"), context.session)?;
        let Some(retained) = self.session_environments.get(&session) else {
            return show_environment_entries(&EMPTY_ENVIRONMENT, &positional, &options);
        };
        show_environment_entries(&retained.inner.lock(), &positional, &options)
    }

    pub(crate) fn format_global_environment_rows(&self) -> Vec<FormatEnvironRow> {
        format_environment_rows(&self.global_environment)
    }

    pub(crate) fn format_session_environment_rows(
        &self,
        session: SessionId,
    ) -> Vec<FormatEnvironRow> {
        self.session_environments
            .get(&session)
            .map(|retained| format_environment_rows(&retained.inner.lock()))
            .unwrap_or_default()
    }

    fn update_environment_names(&self) -> impl Iterator<Item = &str> {
        self.stored_arrays
            .global_session
            .get("update-environment")
            .into_iter()
            .flat_map(|array| array.values().map(String::as_str))
    }

    fn update_environment_names_for_session(&self, session: Option<SessionId>) -> Vec<String> {
        let target = session.map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session);
        self.array_option_readback(target, "update-environment", true)
            .into_iter()
            .flat_map(|(array, _)| array.values().cloned())
            .collect()
    }

    fn seed_session_environment(
        &mut self,
        session: SessionId,
        include_update_environment: bool,
        client_environment: Option<&BTreeMap<RawText, RawText>>,
        overlays: Vec<(String, String)>,
    ) {
        let mut environment = Environment::new();
        if include_update_environment && let Some(client_environment) = client_environment {
            let patterns = self.update_environment_names_for_session(None);
            apply_client_environment_update(&mut environment, &patterns, client_environment);
        }
        for (name, value) in overlays {
            environment.insert(
                name.into(),
                EnvironmentEntry {
                    value: Some(value.into()),
                    hidden: false,
                },
            );
        }
        *self
            .session_environments
            .entry(session)
            .or_default()
            .inner
            .lock() = environment;
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    fn environment_format_context(
        &self,
        session: Option<SessionId>,
        active_session: Option<SessionId>,
        format_client: FormatClient,
    ) -> FormatContext {
        let window = session.and_then(|session| {
            self.state
                .sessions
                .get(&session)
                .map(|session| session.active_window)
        });
        let pane = window.and_then(|window| {
            self.state
                .windows
                .get(&window)
                .map(|window| window.active_pane)
        });
        FormatContext {
            session,
            window,
            pane,
            active_session,
            format_client,
            format_type: FormatType::Pane,
        }
    }

    fn source_file(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        stdin: Option<&RawText>,
        stdin_spent: bool,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("source-file", args)?;
        if positional.is_empty() {
            return Err(ServerError::CommandParse(
                "source-file needs a path".to_owned(),
            ));
        }
        let quiet = options.has("-q");
        let expand_paths = options.has("-F");
        let parse_only = options.has("-n");
        let verbose = options.has("-v");
        let mut source_context = context.clone();
        if let Some(target) = options.value("-t") {
            match self.resolve_pane(Some(target), context.window, context.pane) {
                Ok(pane) => {
                    let target = ExecutionContext::for_pane(&self.state, pane)
                        .expect("resolved pane has an execution context");
                    source_context.retarget(&target);
                }
                Err(_) => source_context.retarget(&ExecutionContext::new(None, None, None)),
            }
        }
        let mut stream = stdin.cloned();
        let carried_a_stream = stdin.is_some() || stdin_spent;
        Ok(Execution {
            output: RawText::default(),
            effects: positional
                .into_iter()
                .map(|path| {
                    let path = if expand_paths {
                        self.expand_pane_format_bytes(
                            &path,
                            &source_context,
                            context.session,
                            context.target_format_client(),
                            hooks,
                        )
                    } else {
                        path.clone()
                    };
                    let stdin = if path == "-" {
                        match stream.take() {
                            Some(bytes) => Some(SourceStream::Bytes(bytes)),
                            None if carried_a_stream => Some(SourceStream::Spent),
                            None => None,
                        }
                    } else {
                        None
                    };
                    MuxEffect::SourceFile {
                        path,
                        quiet,
                        parse_only,
                        verbose,
                        context: source_context.clone(),
                        stdin,
                    }
                })
                .collect(),
        })
    }

    pub fn repair_context(&self, context: &mut ExecutionContext) {
        if let Some(pane) = context.pane
            && let Some(valid) = ExecutionContext::for_pane(&self.state, pane)
        {
            context.retarget(&valid);
            return;
        }
        if let Some(window) = context.window
            && let Some(window_state) = self.state.windows.get(&window)
        {
            context.retarget(&ExecutionContext::new(
                Some(window_state.session),
                Some(window),
                Some(window_state.active_pane),
            ));
            return;
        }
        if let Some(session) = context.session
            && let Some(session_state) = self.state.sessions.get(&session)
            && let Some(window_state) = self.state.windows.get(&session_state.active_window)
        {
            context.retarget(&ExecutionContext::new(
                Some(session),
                Some(session_state.active_window),
                Some(window_state.active_pane),
            ));
            return;
        }
        if let Some((session, window, pane)) = self.state.default_context() {
            context.retarget(&ExecutionContext::new(
                Some(session),
                Some(window),
                Some(pane),
            ));
        } else {
            context.session = None;
            context.window = None;
            context.pane = None;
        }
    }

    pub fn repair_event_context(&self, context: &mut ExecutionContext) {
        self.repair_context(context);
    }
}

fn format_environment_rows(environment: &Environment) -> Vec<FormatEnvironRow> {
    environment
        .iter()
        .map(|(name, entry)| FormatEnvironRow {
            name: name.to_string(),
            value: entry.value.clone().unwrap_or_default(),
            hidden: entry.hidden,
            removed: entry.value.is_none(),
        })
        .collect()
}

#[derive(Default)]
struct ShownOptions {
    lines: Vec<RawText>,
    values: Option<BTreeMap<String, String>>,
    rows: Vec<ShownOptionRow>,
}

struct ShownOptionRow {
    name: String,
    array_key: Option<String>,
    value: String,
    is_string: bool,
    parent: bool,
    has_value: bool,
    fire: Option<HookFire>,
}

impl ShownOptionRow {
    fn context(&self, value_only: bool) -> Vec<(&'static str, String)> {
        let flag = |value: bool| u8::from(value).to_string();
        let user = self.name.starts_with('@');
        let table = exact_tmux_option(&self.name);
        let hook = table.is_some_and(|option| tmux_option_is_hook(option.name));
        vec![
            ("option_name", self.name.clone()),
            ("option_value", self.value.clone()),
            ("option_value_only", flag(value_only)),
            ("option_is_parent", flag(self.parent)),
            (
                "option_is_array",
                flag(table.is_some_and(|option| option.is_array)),
            ),
            ("option_is_string", flag(self.is_string)),
            ("option_is_hook", flag(hook)),
            ("option_is_user", flag(user)),
            ("option_has_value", flag(self.has_value)),
            (
                "option_array_key",
                self.array_key.clone().unwrap_or_default(),
            ),
            ("option_has_array_key", flag(self.array_key.is_some())),
        ]
        .into_iter()
        .chain(self.fire.map(HookFire::context).unwrap_or_default())
        .collect()
    }
}

impl ShownOptions {
    fn new(json: bool) -> Self {
        Self {
            lines: Vec::new(),
            values: json.then(BTreeMap::new),
            rows: Vec::new(),
        }
    }

    fn record(&mut self, name: &str, value: &str) {
        if let Some(values) = &mut self.values {
            values.insert(name.to_owned(), value.to_owned());
        }
    }

    fn row(&mut self, name: &str, value: &str, is_string: bool, parent: bool, has_value: bool) {
        let (name, array_key) = match name.strip_suffix(']').and_then(|name| name.split_once('[')) {
            Some((name, key)) => (name.to_owned(), Some(key.to_owned())),
            None => (name.to_owned(), None),
        };
        self.rows.push(ShownOptionRow {
            name,
            array_key,
            value: value.to_owned(),
            is_string,
            parent,
            has_value,
            fire: None,
        });
    }
}

fn tmux_clean_name(name: &str, kind: &str) -> Result<String, ServerError> {
    if name.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(ServerError::InvalidCommand(format!(
            "invalid {kind} name: {name}"
        )));
    }
    Ok(tmux_vis(name, false))
}

pub(crate) fn parse_window_name(input: &str) -> String {
    let mut name = input.strip_prefix('"').unwrap_or(input);
    if let Some(end) = name.find('"') {
        name = &name[..end];
    }
    name = name.strip_prefix("exec ").unwrap_or(name);
    name = name.trim_start_matches([' ', '-']);
    if let Some(end) = name.find(' ') {
        name = &name[..end];
    }
    let bytes = name.as_bytes();
    let mut end = bytes.len();
    while end > 1
        && !(bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1].is_ascii_punctuation())
    {
        end -= 1;
    }
    let Ok(name) = std::str::from_utf8(&bytes[..end]) else {
        return String::new();
    };
    let name = if name.starts_with('/') {
        let trimmed = name.trim_end_matches('/');
        if trimmed.is_empty() {
            "/"
        } else {
            trimmed.rsplit('/').next().unwrap_or(trimmed)
        }
    } else {
        name
    };
    tmux_vis(name, false)
}

fn tmux_clean_title(title: &str) -> Option<String> {
    if title.bytes().any(|byte| byte.is_ascii_control()) {
        return None;
    }
    Some(tmux_vis(title, false))
}

fn tmux_flag(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn validate_environment_name(name: &RawText) -> Result<(), ServerError> {
    let name = name.as_bytes();
    if name.is_empty() {
        return Err(ServerError::InvalidCommand(
            "empty variable name".to_owned(),
        ));
    }
    if name.contains(&b'=') {
        return Err(ServerError::InvalidCommand(
            "variable name contains =".to_owned(),
        ));
    }
    Ok(())
}

fn extend_job_environment(target: &mut BTreeMap<RawText, Option<RawText>>, source: &Environment) {
    for (name, entry) in source {
        let value = if entry.hidden {
            None
        } else {
            entry.value.clone()
        };
        target.insert(name.clone(), value);
    }
}

fn environment_line(
    name: &RawText,
    entry: &EnvironmentEntry,
    options: &Options,
) -> Option<RawText> {
    if options.has("-h") != entry.hidden {
        return None;
    }
    let name = name.as_bytes();
    let mut line = Vec::new();
    if !options.has("-s") {
        if let Some(value) = &entry.value {
            line.extend_from_slice(name);
            line.push(b'=');
            line.extend_from_slice(value.as_bytes());
        } else {
            line.push(b'-');
            line.extend_from_slice(name);
        }
        return Some(RawText::from_bytes(line));
    }
    if let Some(value) = &entry.value {
        line.extend_from_slice(name);
        line.extend_from_slice(b"=\"");
        line.extend_from_slice(&shell_environment_escape(value.as_bytes()));
        line.extend_from_slice(b"\"; export ");
        line.extend_from_slice(name);
        line.push(b';');
    } else {
        line.extend_from_slice(b"unset ");
        line.extend_from_slice(name);
        line.push(b';');
    }
    Some(RawText::from_bytes(line))
}

fn show_environment_entries(
    environment: &Environment,
    positional: &[RawText],
    options: &Options,
) -> Result<Execution, ServerError> {
    if let Some(name) = positional.first() {
        let entry = environment
            .get(name)
            .ok_or_else(|| ServerError::InvalidCommand(format!("unknown variable: {name}")))?;
        return Ok(Execution::output(
            environment_line(name, entry, options).unwrap_or_default(),
        ));
    }
    let mut output = Vec::new();
    for line in environment
        .iter()
        .filter_map(|(name, entry)| environment_line(name, entry, options))
    {
        if !output.is_empty() {
            output.push(b'\n');
        }
        output.extend_from_slice(line.as_bytes());
    }
    Ok(Execution::output(RawText::from_bytes(output)))
}

fn shell_environment_escape(value: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(value.len());
    for byte in value {
        if matches!(byte, b'$' | b'`' | b'"' | b'\\') {
            escaped.push(b'\\');
        }
        escaped.push(*byte);
    }
    escaped
}

#[must_use]
pub fn tmux_signal_name(value: &str) -> String {
    #[cfg(target_os = "linux")]
    if let Some(signal) = value
        .trim()
        .to_ascii_lowercase()
        .strip_prefix("real-time signal ")
        .and_then(|offset| offset.parse::<i32>().ok())
        .filter(|offset| *offset >= 0)
        .and_then(|offset| nix::libc::SIGRTMIN().checked_add(offset))
        .filter(|signal| *signal <= nix::libc::SIGRTMAX())
    {
        return signal.to_string();
    }
    let name = short_signal_name(value);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use nix::libc;

        let signal = match name.as_str() {
            "hup" => Some(libc::SIGHUP),
            "int" => Some(libc::SIGINT),
            "quit" => Some(libc::SIGQUIT),
            "ill" => Some(libc::SIGILL),
            "trap" => Some(libc::SIGTRAP),
            "abrt" => Some(libc::SIGABRT),
            #[cfg(target_os = "macos")]
            "emt" => Some(libc::SIGEMT),
            "fpe" => Some(libc::SIGFPE),
            "kill" => Some(libc::SIGKILL),
            "bus" => Some(libc::SIGBUS),
            "segv" => Some(libc::SIGSEGV),
            "sys" => Some(libc::SIGSYS),
            "pipe" => Some(libc::SIGPIPE),
            "alrm" => Some(libc::SIGALRM),
            "term" => Some(libc::SIGTERM),
            "urg" => Some(libc::SIGURG),
            "stop" => Some(libc::SIGSTOP),
            "tstp" => Some(libc::SIGTSTP),
            "cont" => Some(libc::SIGCONT),
            "chld" => Some(libc::SIGCHLD),
            "ttin" => Some(libc::SIGTTIN),
            "ttou" => Some(libc::SIGTTOU),
            "io" => Some(libc::SIGIO),
            "xcpu" => Some(libc::SIGXCPU),
            "xfsz" => Some(libc::SIGXFSZ),
            "vtalrm" => Some(libc::SIGVTALRM),
            "prof" => Some(libc::SIGPROF),
            "winch" => Some(libc::SIGWINCH),
            #[cfg(target_os = "macos")]
            "info" => Some(libc::SIGINFO),
            "usr1" => Some(libc::SIGUSR1),
            "usr2" => Some(libc::SIGUSR2),
            #[cfg(target_os = "linux")]
            "pwr" => Some(libc::SIGPWR),
            #[cfg(target_os = "linux")]
            "stkflt" => Some(libc::SIGSTKFLT),
            _ => name.parse::<i32>().ok(),
        };
        let Some(signal) = signal else {
            return name;
        };
        #[cfg(target_os = "linux")]
        return signal.to_string();
        #[cfg(target_os = "macos")]
        return macos_signal_name(signal);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    name
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn macos_signal_name(signal: i32) -> String {
    unsafe extern "C" {
        static sys_signame: [*const std::ffi::c_char; 32];
    }
    if signal > 0 && signal < 32 {
        let name = unsafe { sys_signame[signal as usize] };
        if !name.is_null() {
            return unsafe { std::ffi::CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned();
        }
    }
    signal.to_string()
}

fn short_signal_name(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    let lower = value.to_ascii_lowercase();
    let short = lower.strip_prefix("sig").unwrap_or(&lower);
    if matches!(
        short,
        "hup"
            | "int"
            | "quit"
            | "ill"
            | "trap"
            | "abrt"
            | "emt"
            | "fpe"
            | "kill"
            | "bus"
            | "segv"
            | "sys"
            | "pipe"
            | "alrm"
            | "term"
            | "urg"
            | "stop"
            | "tstp"
            | "cont"
            | "chld"
            | "ttin"
            | "ttou"
            | "io"
            | "xcpu"
            | "xfsz"
            | "vtalrm"
            | "prof"
            | "winch"
            | "info"
            | "usr1"
            | "usr2"
            | "pwr"
            | "stkflt"
    ) {
        return short.to_owned();
    }
    for (description, name) in [
        ("hangup", "hup"),
        ("interrupt", "int"),
        ("quit", "quit"),
        ("illegal instruction", "ill"),
        ("trace/bpt trap", "trap"),
        ("trace/breakpoint trap", "trap"),
        ("trace trap", "trap"),
        ("abort trap", "abrt"),
        ("aborted", "abrt"),
        ("emt trap", "emt"),
        ("floating point exception", "fpe"),
        ("killed", "kill"),
        ("bus error", "bus"),
        ("segmentation fault", "segv"),
        ("bad system call", "sys"),
        ("broken pipe", "pipe"),
        ("alarm clock", "alrm"),
        ("terminated", "term"),
        ("urgent i/o condition", "urg"),
        ("stopped (signal)", "stop"),
        ("stopped (tty input)", "ttin"),
        ("stopped (tty output)", "ttou"),
        ("stopped", "tstp"),
        ("continued", "cont"),
        ("child exited", "chld"),
        ("i/o possible", "io"),
        ("cpu time limit exceeded", "xcpu"),
        ("cputime limit exceeded", "xcpu"),
        ("file size limit exceeded", "xfsz"),
        ("filesize limit exceeded", "xfsz"),
        ("virtual timer expired", "vtalrm"),
        ("profiling timer expired", "prof"),
        ("window size changes", "winch"),
        ("window changed", "winch"),
        ("information request", "info"),
        ("user defined signal 1", "usr1"),
        ("user defined signal 2", "usr2"),
        ("power failure", "pwr"),
        ("stack fault", "stkflt"),
    ] {
        if lower.starts_with(description) {
            return name.to_owned();
        }
    }
    let number = lower
        .split(|character: char| !character.is_ascii_digit())
        .rfind(|part| !part.is_empty())
        .map(str::to_owned);
    number.unwrap_or(lower)
}

#[derive(Clone, Debug, Default)]
struct Options {
    flags: Vec<String>,
    values: Vec<(String, String)>,
}

impl Options {
    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|candidate| candidate == flag)
    }

    fn count(&self, flag: &str) -> usize {
        self.flags
            .iter()
            .filter(|candidate| *candidate == flag)
            .count()
    }

    fn value(&self, option: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find_map(|(name, value)| (name == option).then_some(value.as_str()))
    }

    fn values<'a>(&'a self, option: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.values
            .iter()
            .filter_map(move |(name, value)| (name == option).then_some(value.as_str()))
    }
}

fn spawn_environment(options: &Options) -> Vec<(String, String)> {
    options
        .values("-e")
        .filter_map(|value| value.split_once('='))
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

fn session_creation_environment(options: &Options) -> Vec<(String, String)> {
    options
        .values("-e")
        .filter_map(|value| value.split_once('='))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

enum EnvironmentPattern {
    Name(String),
    Glob(GlobPattern),
}

type CompiledPatterns = Arc<[(String, EnvironmentPattern)]>;

fn compiled_update_environment(patterns: &[String]) -> CompiledPatterns {
    static COMPILED: std::sync::LazyLock<
        parking_lot::Mutex<Option<(Vec<String>, CompiledPatterns)>>,
    > = std::sync::LazyLock::new(parking_lot::Mutex::default);
    let mut cached = COMPILED.lock();
    if let Some((key, compiled)) = cached.as_ref()
        && key.as_slice() == patterns
    {
        return Arc::clone(compiled);
    }
    let compiled: CompiledPatterns = patterns
        .iter()
        .map(|pattern| {
            let glob = GlobPattern::new(pattern);
            let compiled = glob
                .literal()
                .map_or(EnvironmentPattern::Glob(glob), EnvironmentPattern::Name);
            (pattern.clone(), compiled)
        })
        .collect();
    *cached = Some((patterns.to_vec(), Arc::clone(&compiled)));
    compiled
}

fn apply_client_environment_update(
    environment: &mut Environment,
    patterns: &[String],
    client_environment: &BTreeMap<RawText, RawText>,
) {
    for (pattern, compiled) in compiled_update_environment(patterns).iter() {
        let matches = match compiled {
            EnvironmentPattern::Name(name) => client_environment
                .range::<str, _>((
                    std::ops::Bound::Included(name.as_str()),
                    std::ops::Bound::Included(name.as_str()),
                ))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>(),
            EnvironmentPattern::Glob(glob) => client_environment
                .iter()
                .filter(|(name, _)| glob.matches(name))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>(),
        };
        if matches.is_empty() {
            environment
                .entry(pattern.clone().into())
                .and_modify(|entry| entry.value = None)
                .or_insert(EnvironmentEntry {
                    value: None,
                    hidden: false,
                });
        } else {
            for (name, value) in matches {
                environment.insert(
                    name,
                    EnvironmentEntry {
                        value: Some(value),
                        hidden: false,
                    },
                );
            }
        }
    }
}

fn command_prompt_mode(options: &Options) -> CommandPromptMode {
    if options.has("-1") {
        CommandPromptMode::Single
    } else if options.has("-N") {
        CommandPromptMode::Numeric
    } else if options.has("-i") {
        CommandPromptMode::Incremental
    } else if options.has("-k") {
        CommandPromptMode::Key
    } else if options.has("-e") {
        CommandPromptMode::BackspaceExit
    } else {
        CommandPromptMode::Text
    }
}

fn parse_command_options(
    command: &str,
    args: &[RawText],
) -> Result<(Options, Vec<RawText>), ServerError> {
    let spec = command_spec(command).expect("executable command has catalog metadata");
    let (options, positional) = parse_options_for_spec(args, spec)?;
    spec.validate_positional_minimum(positional.len())?;
    if !NATIVE_COMMAND_NAMES.contains(&spec.name) {
        spec.validate_positional_maximum(positional.len())?;
    }
    validate_options(command, spec, &options)?;
    Ok((options, positional))
}

fn parse_display_message_options(args: &[RawText]) -> Result<(Options, Vec<RawText>), ServerError> {
    match args {
        [print, payload] if print == "-p" && !payload.starts_with('-') => Ok((
            Options {
                flags: vec!["-p".to_owned()],
                values: Vec::new(),
            },
            vec![payload.clone()],
        )),
        _ => parse_command_options("display-message", args),
    }
}

#[cfg(test)]
mod display_message_options_tests;

fn parse_options_for_spec(
    args: &[RawText],
    spec: &zz_protocol::CommandSpec,
) -> Result<(Options, Vec<RawText>), ServerError> {
    if spec.uses_tmux_option_grammar() {
        let parsed = parse_tmux_options(spec, args)?;
        let mut options = Options::default();
        for option in parsed.options {
            match option {
                zz_protocol::TmuxOption::Flag(name) => options.flags.push(name.to_owned()),
                zz_protocol::TmuxOption::Value(name, value) => {
                    options.values.push((name.to_owned(), value.to_owned()));
                }
            }
        }
        return Ok((options, parsed.positionals.to_vec()));
    }
    let value_options = spec
        .options
        .iter()
        .filter_map(|option| option.value.map(|_| option.name))
        .collect::<Vec<_>>();
    let attached_options = spec
        .options
        .iter()
        .filter_map(|option| option.attached_value.then_some(option.name))
        .collect::<Vec<_>>();
    parse_options(args, &value_options, &attached_options)
        .map_err(|error| spec.classify_usage_error(error))
}

fn validate_options(
    command: &str,
    spec: &zz_protocol::CommandSpec,
    options: &Options,
) -> Result<(), ServerError> {
    validate_options_allowing_unsupported(command, spec, options, None)
}

fn validate_options_allowing_unsupported(
    command: &str,
    spec: &zz_protocol::CommandSpec,
    options: &Options,
    allowed: Option<&str>,
) -> Result<(), ServerError> {
    for name in options
        .flags
        .iter()
        .map(String::as_str)
        .chain(options.values.iter().map(|(name, _)| name.as_str()))
    {
        let Some(option) = spec.option(name) else {
            return Err(spec.parse_error(format!("{command} does not support {name}")));
        };
        if option.unsupported && allowed != Some(name) {
            return Err(ServerError::UnsupportedCommand(format!("{command} {name}")));
        }
    }
    Ok(())
}

fn parse_options(
    args: &[RawText],
    value_options: &[&str],
    attached_options: &[&str],
) -> Result<(Options, Vec<RawText>), ServerError> {
    let mut options = Options::default();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if !argument.starts_with('-') || argument == "-" {
            break;
        }
        index += 1;
        if argument == "--" {
            break;
        }
        if let Some(long) = argument.strip_prefix("--") {
            let (name, attached) = long
                .split_once('=')
                .map_or((long, None), |(name, value)| (name, Some(value)));
            let name = format!("--{name}");
            if value_options.contains(&name.as_str()) {
                let value = match attached {
                    Some(value) if !value.is_empty() => value.to_owned(),
                    Some(_) => {
                        options.flags.push(argument.to_string());
                        continue;
                    }
                    None => {
                        let value = required_arg(args, index, &name)?.to_owned();
                        index += 1;
                        value
                    }
                };
                options.values.push((name, value));
            } else {
                options.flags.push(argument.to_string());
            }
            continue;
        }
        let mut cluster = argument[1..].chars();
        while let Some(character) = cluster.next() {
            let name = format!("-{character}");
            if attached_options.contains(&name.as_str()) {
                let attached = cluster.as_str();
                if attached.is_empty() {
                    options.flags.push(name);
                } else {
                    options.values.push((name, attached.to_owned()));
                }
                break;
            }
            if !value_options.contains(&name.as_str()) {
                options.flags.push(name);
                continue;
            }
            let attached = cluster.as_str();
            let value = if attached.is_empty() {
                let value = required_arg(args, index, &name)?.to_owned();
                index += 1;
                value
            } else {
                attached.to_owned()
            };
            options.values.push((name, value));
            break;
        }
    }
    Ok((options, args[index..].to_vec()))
}

fn required_arg<'a>(
    args: &'a [RawText],
    index: usize,
    option: &str,
) -> Result<&'a str, ServerError> {
    args.get(index)
        .map(RawText::as_str)
        .ok_or_else(|| ServerError::CommandParse(format!("{option} requires an argument")))
}

fn chooser_preview(options: &Options) -> ChooserPreviewSize {
    match options.count("-N") {
        0 => ChooserPreviewSize::Normal,
        1 => ChooserPreviewSize::Off,
        _ => ChooserPreviewSize::Big,
    }
}

fn chooser_command_template(
    invocation: &CommandInvocation,
    positional_start: usize,
    positional: &[RawText],
) -> Option<String> {
    positional.first().map(|template| {
        if invocation.argument_is_command_block(positional_start) {
            crate::parser::command_block_body(template)
                .unwrap_or(template)
                .trim()
                .replace('\n', " ;; ")
        } else {
            template.to_string()
        }
    })
}

fn reject_positionals(command: &str, positional: &[RawText]) -> Result<(), ServerError> {
    if let Some(argument) = positional.first() {
        Err(ServerError::CommandParse(format!(
            "{command} does not take positional arguments: {argument}"
        )))
    } else {
        Ok(())
    }
}

fn percentage_value(value: &str, minimum: i64, maximum: i64, current: i64) -> Result<i64, String> {
    let strtonum = |value: &str, minimum: i64, maximum: i64| {
        parse_strtonum(value, minimum, maximum, "").map_err(|error| match error {
            ServerError::InvalidCommand(cause) => cause.trim_start().to_owned(),
            other => other.to_string(),
        })
    };
    if let Some(percent) = value.strip_suffix('%') {
        let value = current * strtonum(percent, 0, 1000)? / 100;
        if value < minimum {
            return Err("too small".to_owned());
        }
        if value > maximum {
            return Err("too large".to_owned());
        }
        return Ok(value);
    }
    strtonum(value, minimum, maximum)
}

fn parse_strtonum(
    value: &str,
    minimum: i64,
    maximum: i64,
    label: &str,
) -> Result<i64, ServerError> {
    let trimmed = value.trim_start();
    let (negative, digits) = trimmed.strip_prefix('-').map_or_else(
        || (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
        |digits| (true, digits),
    );
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ServerError::InvalidCommand(format!("{label} invalid")));
    }
    let magnitude = digits.parse::<i128>().unwrap_or(i128::MAX);
    let number = if negative { -magnitude } else { magnitude };
    if number < i128::from(minimum) {
        return Err(ServerError::InvalidCommand(format!("{label} too small")));
    }
    if number > i128::from(maximum) {
        return Err(ServerError::InvalidCommand(format!("{label} too large")));
    }
    Ok(i64::try_from(number).expect("the value was bounded to i64 limits"))
}

fn parse_resize_adjustment(value: &str) -> Result<i32, ServerError> {
    value
        .parse::<i32>()
        .map_err(|_| ServerError::InvalidCommand("adjustment invalid".to_owned()))
}

fn parse_resize_window_adjustment(value: &str) -> Result<u32, ServerError> {
    parse_strtonum(value, 1, i64::from(i32::MAX), "adjustment")
        .map(|value| u32::try_from(value).expect("window adjustment is positive and bounded"))
}

fn parse_window_extent(value: &str, dimension: &str) -> Result<u16, ServerError> {
    parse_strtonum(value, 1, i64::from(PANE_MAXIMUM), dimension)
        .map(|value| u16::try_from(value).expect("window extent is bounded to u16"))
}

fn parse_resize_size(value: &str, extent: u16, dimension: &str) -> Result<u16, ServerError> {
    let (number, maximum) = value
        .strip_suffix('%')
        .map_or((value, i64::from(PANE_MAXIMUM)), |percentage| {
            (percentage, 1_000)
        });
    let number = number
        .parse::<i64>()
        .map_err(|_| ServerError::InvalidCommand(format!("{dimension} invalid")))?;
    if number < 0 {
        return Err(ServerError::InvalidCommand(format!(
            "{dimension} too small"
        )));
    }
    if number > maximum {
        return Err(ServerError::InvalidCommand(format!(
            "{dimension} too large"
        )));
    }
    let cells = if value.ends_with('%') {
        u64::from(extent) * u64::try_from(number).expect("percentage is nonnegative") / 100
    } else {
        u64::try_from(number).expect("pane size is nonnegative")
    };
    u16::try_from(cells)
        .ok()
        .filter(|cells| *cells <= PANE_MAXIMUM)
        .ok_or_else(|| ServerError::InvalidCommand(format!("{dimension} too large")))
}

fn exactly_one_argument<'a>(
    command: &str,
    positional: &'a [RawText],
) -> Result<&'a str, ServerError> {
    if positional.len() == 1 {
        Ok(positional[0].as_str())
    } else {
        Err(ServerError::CommandParse(format!(
            "{command} requires exactly one new name"
        )))
    }
}

impl MuxEngine {
    #[must_use]
    pub fn format_pane_kind(&self, pane: &str) -> Option<&'static str> {
        self.state
            .pane(pane.parse().ok()?)
            .map(|pane| pane_kind_name(&pane.kind))
    }

    #[must_use]
    pub fn format_browser_url(&self, pane: &str) -> Option<&str> {
        match &self.state.pane(pane.parse().ok()?)?.kind {
            PaneKind::Browser(browser) => Some(browser.url()),
            _ => None,
        }
    }

    #[must_use]
    pub fn format_facts(&self) -> FormatFacts {
        let mut pane_kinds = BTreeMap::new();
        let mut browser_urls = BTreeMap::new();
        let mut pane_windows = BTreeMap::new();
        let agent_chats = self
            .agent_chats
            .iter()
            .map(|(pane, facts)| (pane.to_string(), facts.clone()))
            .collect();
        let agent_parents = self
            .agent_parents
            .iter()
            .map(|(child, parent)| (child.to_string(), parent.to_string()))
            .collect();
        for (window, entry) in &self.state.windows {
            for (pane, entry) in &entry.panes {
                pane_kinds.insert(pane.to_string(), pane_kind_name(&entry.kind));
                if let PaneKind::Browser(browser) = &entry.kind {
                    browser_urls.insert(pane.to_string(), browser.url().to_owned());
                }
                pane_windows.insert(pane.to_string(), window.to_string());
            }
        }
        fn keyed<K: std::fmt::Display>(
            options: &BTreeMap<K, SharedUserOptions>,
        ) -> BTreeMap<String, SharedUserOptions> {
            options
                .iter()
                .filter(|(_, values)| !values.is_empty())
                .map(|(id, values)| (id.to_string(), Arc::clone(values)))
                .collect()
        }
        FormatFacts {
            pane_kinds,
            browser_urls,
            agent_chats,
            agent_parents,
            pane_windows,
            server: Arc::clone(&self.server_user_options),
            global_session: Arc::clone(&self.global_session_user_options),
            sessions: keyed(&self.session_user_options),
            global_window: Arc::clone(&self.global_window_user_options),
            windows: keyed(&self.window_user_options),
            panes: keyed(&self.pane_user_options),
        }
    }
}

struct WindowDestination {
    session: SessionId,
    index: Option<u32>,
}

fn session_named(state: &MuxState, name: &str) -> Option<SessionId> {
    state
        .sessions
        .values()
        .find(|session| session.name == name)
        .map(|session| session.id)
}

fn window_index(state: &MuxState, window: WindowId) -> Result<u32, ServerError> {
    state
        .windows
        .get(&window)
        .map(|window| window.index)
        .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
}

fn session_active_window(state: &MuxState, session: SessionId) -> Result<WindowId, ServerError> {
    state
        .sessions
        .get(&session)
        .map(|session| session.active_window)
        .ok_or_else(|| ServerError::MissingTarget(session.to_string()))
}

fn window_active_pane(state: &MuxState, window: WindowId) -> Result<PaneId, ServerError> {
    state
        .windows
        .get(&window)
        .map(|window| window.active_pane)
        .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
}

fn shell_command_positional(positional: &[RawText]) -> Option<Vec<String>> {
    (!positional.is_empty()).then(|| positional.iter().map(ToString::to_string).collect())
}

fn key_token(value: &str) -> KeyToken {
    if let Some(name) = canonical_named_key(value) {
        return KeyToken::Named(name.to_owned());
    }
    const NAMED: &[&str] = &[
        "Enter", "Escape", "Space", "Tab", "BSpace", "Up", "Down", "Left", "Right", "Home", "End",
        "PPage", "NPage", "DC", "IC",
    ];
    if NAMED.contains(&value)
        || value.starts_with("C-")
        || value.starts_with("M-")
        || value
            .strip_prefix('F')
            .is_some_and(|number| number.parse::<u8>().is_ok())
    {
        KeyToken::Named(value.to_owned())
    } else {
        KeyToken::Literal(value.to_owned())
    }
}

fn hex_key_token(value: &str) -> Option<KeyToken> {
    let digits = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let code = u32::from_str_radix(digits, 16).ok()?;
    match code {
        0x00..=0x7f => Some(KeyToken::Literal(
            char::from_u32(code).expect("ascii range").to_string(),
        )),
        0x80..=0xff => Some(KeyToken::Raw(u8::try_from(code).expect("byte range"))),
        _ => None,
    }
}

fn parse_repeat_count(value: &str) -> Result<u32, ServerError> {
    u32::try_from(parse_strtonum(
        value,
        1,
        i64::from(u32::MAX),
        "repeat count",
    )?)
    .map_err(|_| ServerError::InvalidCommand("repeat count too large".to_owned()))
}

#[must_use]
pub fn copy_mode_action_is_read_only_safe(action: &CopyModeAction) -> bool {
    matches!(
        action,
        CopyModeAction::Left
            | CopyModeAction::Right
            | CopyModeAction::Up
            | CopyModeAction::Down
            | CopyModeAction::PageUp
            | CopyModeAction::PageDown
            | CopyModeAction::HalfPageUp
            | CopyModeAction::HalfPageDown
            | CopyModeAction::Top
            | CopyModeAction::Bottom
            | CopyModeAction::TopLine
            | CopyModeAction::MiddleLine
            | CopyModeAction::BottomLine
            | CopyModeAction::StartOfLine
            | CopyModeAction::BackToIndentation
            | CopyModeAction::EndOfLine
            | CopyModeAction::NextWord
            | CopyModeAction::PreviousWord
            | CopyModeAction::NextWordEnd
            | CopyModeAction::NextParagraph
            | CopyModeAction::PreviousParagraph
            | CopyModeAction::NextPrompt { .. }
            | CopyModeAction::PreviousPrompt { .. }
            | CopyModeAction::SetMark
            | CopyModeAction::JumpToMark
            | CopyModeAction::MouseCursor(_)
            | CopyModeAction::Cancel
            | CopyModeAction::NextSpace
            | CopyModeAction::PreviousSpace
            | CopyModeAction::NextSpaceEnd
            | CopyModeAction::ScrollUp
            | CopyModeAction::ScrollDown
            | CopyModeAction::ScrollMiddle
            | CopyModeAction::ScrollTop
            | CopyModeAction::ScrollBottom
            | CopyModeAction::CursorCentreVertical
            | CopyModeAction::CursorCentreHorizontal
            | CopyModeAction::TogglePosition
            | CopyModeAction::RefreshOn
            | CopyModeAction::RefreshOff
            | CopyModeAction::RefreshToggle
            | CopyModeAction::RefreshNow
            | CopyModeAction::LineNumbersOn { .. }
            | CopyModeAction::LineNumbersOff
            | CopyModeAction::LineNumbersToggle { .. }
            | CopyModeAction::RefreshRevision
            | CopyModeAction::RecentreTopBottom
            | CopyModeAction::NextMatchingBracket
            | CopyModeAction::PreviousMatchingBracket
            | CopyModeAction::GotoLine { .. }
            | CopyModeAction::PageDownScrollExit
            | CopyModeAction::HalfPageDownScrollExit
            | CopyModeAction::ScrollDownAndCancel
            | CopyModeAction::CursorDownAndCancel
    )
}

pub fn send_keys_is_read_only_safe(args: &[RawText]) -> Result<bool, ServerError> {
    let spec = command_spec("send-keys").expect("send-keys has catalog metadata");
    let (unvalidated_options, _) = parse_options_for_spec(args, spec)?;
    if !unvalidated_options.has("-X") {
        return Ok(false);
    }
    let (_, positional) = parse_command_options("send-keys", args)?;
    let Some(command) = positional.first() else {
        return Ok(true);
    };
    let probe = copy_mode_probe_action(command);
    if probe.is_none()
        && let Some(pinned) = pinned_copy_action(command)
    {
        return Ok(pinned.read_only);
    }
    if let Ok(Some(action)) = copy_mode_action(command, &positional[1..], SetClipboard::Off, "") {
        return Ok(copy_mode_action_is_read_only_safe(&action));
    }
    Ok(probe
        .as_ref()
        .is_none_or(copy_mode_action_is_read_only_safe))
}

pub fn send_keys_target_client(args: &[RawText]) -> Result<Option<String>, ServerError> {
    let spec = command_spec("send-keys").expect("send-keys has catalog metadata");
    let (options, _) = parse_options_for_spec(args, spec)?;
    Ok(options.value("-c").map(str::to_owned))
}

pub(crate) fn copy_mode_probe_action(command: &str) -> Option<CopyModeAction> {
    let arguments: Vec<RawText> = match command {
        "goto-line" => vec!["1".into()],
        "jump-forward" | "jump-backward" | "jump-to-forward" | "jump-to-backward" => {
            vec!["x".into()]
        }
        "search-forward-incremental" | "search-backward-incremental" => vec!["=x".into()],
        _ => Vec::new(),
    };
    copy_mode_action(command, &arguments, SetClipboard::Off, "")
        .ok()
        .flatten()
}

fn copy_mode_action(
    command: &str,
    arguments: &[RawText],
    set_clipboard: SetClipboard,
    copy_command: &str,
) -> Result<Option<CopyModeAction>, ServerError> {
    let Some((options, arguments)) = copy_mode_action_options(command, arguments) else {
        return Ok(None);
    };
    let output = options.has("-o");
    let action = match command {
        "cursor-left" => Some(CopyModeAction::Left),
        "cursor-right" => Some(CopyModeAction::Right),
        "cursor-up" => Some(CopyModeAction::Up),
        "cursor-down" => Some(CopyModeAction::Down),
        "page-up" => Some(CopyModeAction::PageUp),
        "page-down" => Some(CopyModeAction::PageDown),
        "page-down-and-cancel" => Some(CopyModeAction::PageDownScrollExit),
        "halfpage-up" => Some(CopyModeAction::HalfPageUp),
        "halfpage-down" => Some(CopyModeAction::HalfPageDown),
        "halfpage-down-and-cancel" => Some(CopyModeAction::HalfPageDownScrollExit),
        "cursor-down-and-cancel" => Some(CopyModeAction::CursorDownAndCancel),
        "history-top" => Some(CopyModeAction::Top),
        "history-bottom" => Some(CopyModeAction::Bottom),
        "top-line" => Some(CopyModeAction::TopLine),
        "middle-line" => Some(CopyModeAction::MiddleLine),
        "bottom-line" => Some(CopyModeAction::BottomLine),
        "start-of-line" => Some(CopyModeAction::StartOfLine),
        "back-to-indentation" => Some(CopyModeAction::BackToIndentation),
        "end-of-line" => Some(CopyModeAction::EndOfLine),
        "next-word" => Some(CopyModeAction::NextWord),
        "previous-word" => Some(CopyModeAction::PreviousWord),
        "next-word-end" => Some(CopyModeAction::NextWordEnd),
        "next-space" => Some(CopyModeAction::NextSpace),
        "previous-space" => Some(CopyModeAction::PreviousSpace),
        "next-space-end" => Some(CopyModeAction::NextSpaceEnd),
        "scroll-up" => Some(CopyModeAction::ScrollUp),
        "scroll-down" => Some(CopyModeAction::ScrollDown),
        "scroll-down-and-cancel" => Some(CopyModeAction::ScrollDownAndCancel),
        "scroll-middle" => Some(CopyModeAction::ScrollMiddle),
        "scroll-top" => Some(CopyModeAction::ScrollTop),
        "scroll-bottom" => Some(CopyModeAction::ScrollBottom),
        "cursor-centre-vertical" => Some(CopyModeAction::CursorCentreVertical),
        "cursor-centre-horizontal" => Some(CopyModeAction::CursorCentreHorizontal),
        "toggle-position" => Some(CopyModeAction::TogglePosition),
        "refresh-on" => Some(CopyModeAction::RefreshOn),
        "refresh-off" => Some(CopyModeAction::RefreshOff),
        "refresh-toggle" => Some(CopyModeAction::RefreshToggle),
        "refresh-now" => Some(CopyModeAction::RefreshNow),
        "line-numbers-on" => Some(CopyModeAction::LineNumbersOn { option_off: true }),
        "line-numbers-off" => Some(CopyModeAction::LineNumbersOff),
        "line-numbers-toggle" => Some(CopyModeAction::LineNumbersToggle { option_off: true }),
        "recentre-top-bottom" => Some(CopyModeAction::RecentreTopBottom),
        "scroll-exit-on" => Some(CopyModeAction::ScrollExitOn),
        "scroll-exit-off" => Some(CopyModeAction::ScrollExitOff),
        "scroll-exit-toggle" => Some(CopyModeAction::ScrollExitToggle),
        "goto-line" => copy_goto_line_action(&arguments),
        "next-matching-bracket" => Some(CopyModeAction::NextMatchingBracket),
        "previous-matching-bracket" => Some(CopyModeAction::PreviousMatchingBracket),
        "search-forward-cursor-word" => Some(CopyModeAction::SearchCursorWord {
            direction: SearchDirection::Forward,
        }),
        "search-backward-cursor-word" => Some(CopyModeAction::SearchCursorWord {
            direction: SearchDirection::Backward,
        }),
        "next-paragraph" => Some(CopyModeAction::NextParagraph),
        "previous-paragraph" => Some(CopyModeAction::PreviousParagraph),
        "next-prompt" => Some(CopyModeAction::NextPrompt { output }),
        "previous-prompt" => Some(CopyModeAction::PreviousPrompt { output }),
        "search-again" => Some(CopyModeAction::SearchAgain { reverse: false }),
        "search-reverse" => Some(CopyModeAction::SearchAgain { reverse: true }),
        "search-forward" | "search-backward" | "search-forward-text" | "search-backward-text" => {
            Some(copy_search_action(command, arguments.first()))
        }
        "search-forward-incremental" | "search-backward-incremental" => {
            copy_incremental_search_action(command, arguments.first())
        }
        "begin-selection" => Some(CopyModeAction::StartSelection),
        "select-word" => Some(CopyModeAction::SelectWord),
        "select-line" => Some(CopyModeAction::SelectLine),
        "selection-mode" => copy_selection_mode(arguments.first().map(RawText::as_str))
            .map(CopyModeAction::SelectionMode),
        "clear-selection" => Some(CopyModeAction::ClearSelection),
        "stop-selection" => Some(CopyModeAction::StopSelection),
        "clear-selection-or-cancel" => Some(CopyModeAction::ClearSelectionOrCancel),
        "rectangle-toggle" => Some(CopyModeAction::ToggleRectangle),
        "rectangle-on" => Some(CopyModeAction::RectangleOn),
        "rectangle-off" => Some(CopyModeAction::RectangleOff),
        "other-end" => Some(CopyModeAction::OtherEnd),
        "set-mark" => Some(CopyModeAction::SetMark),
        "jump-to-mark" => Some(CopyModeAction::JumpToMark),
        "jump-forward" => copy_jump_action(&arguments, CopyJumpDirection::Forward, false),
        "jump-backward" => copy_jump_action(&arguments, CopyJumpDirection::Backward, false),
        "jump-to-forward" => copy_jump_action(&arguments, CopyJumpDirection::Forward, true),
        "jump-to-backward" => copy_jump_action(&arguments, CopyJumpDirection::Backward, true),
        "jump-again" => Some(CopyModeAction::RepeatJump { reverse: false }),
        "jump-reverse" => Some(CopyModeAction::RepeatJump { reverse: true }),
        "copy-selection" => Some(copy_selection_action(
            &arguments,
            &options,
            set_clipboard,
            true,
            false,
        )),
        "copy-selection-no-clear" => Some(copy_selection_action(
            &arguments,
            &options,
            set_clipboard,
            false,
            false,
        )),
        "copy-selection-and-cancel" => Some(copy_selection_action(
            &arguments,
            &options,
            set_clipboard,
            true,
            true,
        )),
        "append-selection" | "append-selection-and-cancel" => {
            Some(CopyModeAction::copy_selection(CopyModeCopy {
                request_id: 0,
                clipboard: false,
                buffer: Some(PasteBufferAction::Append),
                pipe: None,
                clear_selection: true,
                cancel: command.ends_with("-and-cancel"),
            }))
        }
        "copy-pipe" | "copy-pipe-no-clear" | "copy-pipe-and-cancel" => Some(copy_pipe_action(
            command,
            &arguments,
            &options,
            set_clipboard,
            copy_command,
            true,
        )?),
        "pipe" | "pipe-no-clear" | "pipe-and-cancel" => Some(copy_pipe_action(
            command,
            &arguments,
            &options,
            set_clipboard,
            copy_command,
            false,
        )?),
        "copy-pipe-end-of-line" | "copy-pipe-end-of-line-and-cancel" => {
            Some(copy_end_of_line_action(copy_pipe_action(
                command,
                &arguments,
                &options,
                set_clipboard,
                copy_command,
                true,
            )?))
        }
        "copy-pipe-line" | "copy-pipe-line-and-cancel" => Some(copy_line_action(copy_pipe_action(
            command,
            &arguments,
            &options,
            set_clipboard,
            copy_command,
            true,
        )?)),
        "copy-line" | "copy-line-and-cancel" => Some(copy_line_action(copy_selection_action(
            &arguments,
            &options,
            set_clipboard,
            true,
            command.ends_with("-and-cancel"),
        ))),
        "copy-end-of-line" | "copy-end-of-line-and-cancel" => {
            Some(copy_end_of_line_action(copy_selection_action(
                &arguments,
                &options,
                set_clipboard,
                true,
                command.ends_with("-and-cancel"),
            )))
        }
        "cancel" => Some(CopyModeAction::Cancel),
        _ => None,
    };
    Ok(action)
}

fn send_keys_search_string_index(arguments: &[RawText]) -> Option<usize> {
    let mut index = 0;
    while let Some(argument) = arguments.get(index) {
        if argument == "--" {
            return Some(index + 1);
        }
        if argument.starts_with('-') && argument != "-" {
            index += 1;
            continue;
        }
        return Some(index);
    }
    None
}

const SEND_KEYS_FORMAT_EXPANDED_ACTIONS: &[&str] = &[
    "search-backward",
    "search-backward-text",
    "search-forward",
    "search-forward-text",
];

fn copy_search_action(command: &str, argument: Option<&RawText>) -> CopyModeAction {
    CopyModeAction::Search(Box::new(CopyModeSearch {
        text: argument.map(ToString::to_string).unwrap_or_default(),
        direction: if command.starts_with("search-forward") {
            SearchDirection::Forward
        } else {
            SearchDirection::Backward
        },
        regex: !command.ends_with("-text"),
        incremental: false,
    }))
}

fn copy_incremental_search_action(
    command: &str,
    argument: Option<&RawText>,
) -> Option<CopyModeAction> {
    let argument = argument?;
    let mut characters = argument.chars();
    let prefix = characters.next()?;
    let forward_spelling = command.starts_with("search-forward");
    let direction = match prefix {
        '=' => {
            if forward_spelling {
                SearchDirection::Forward
            } else {
                SearchDirection::Backward
            }
        }
        '+' => SearchDirection::Forward,
        '-' => SearchDirection::Backward,
        _ => return None,
    };
    Some(CopyModeAction::Search(Box::new(CopyModeSearch {
        text: characters.as_str().to_owned(),
        direction,
        regex: false,
        incremental: true,
    })))
}

fn copy_mode_action_options(
    command: &str,
    arguments: &[RawText],
) -> Option<(Options, Vec<RawText>)> {
    let (options, positional) = parse_options(arguments, &[], &[]).ok()?;
    let allowed = match command {
        "copy-selection"
        | "copy-selection-no-clear"
        | "copy-selection-and-cancel"
        | "copy-pipe"
        | "copy-pipe-no-clear"
        | "copy-pipe-and-cancel"
        | "copy-pipe-end-of-line"
        | "copy-pipe-end-of-line-and-cancel"
        | "copy-end-of-line"
        | "copy-end-of-line-and-cancel"
        | "copy-line"
        | "copy-line-and-cancel"
        | "copy-pipe-line"
        | "copy-pipe-line-and-cancel" => &["-C", "-P"][..],
        "next-prompt" | "previous-prompt" => &["-o"][..],
        _ => &[],
    };
    if options
        .flags
        .iter()
        .any(|flag| !allowed.contains(&flag.as_str()))
    {
        return None;
    }
    let (minimum, maximum) = match command {
        "goto-line"
        | "jump-forward"
        | "jump-backward"
        | "jump-to-forward"
        | "jump-to-backward"
        | "search-forward-incremental"
        | "search-backward-incremental" => (1, 1),
        "copy-selection"
        | "copy-selection-no-clear"
        | "copy-selection-and-cancel"
        | "copy-end-of-line"
        | "copy-end-of-line-and-cancel"
        | "copy-line"
        | "copy-line-and-cancel"
        | "pipe"
        | "pipe-no-clear"
        | "pipe-and-cancel"
        | "search-forward"
        | "search-backward"
        | "search-forward-text"
        | "search-backward-text"
        | "selection-mode" => (0, 1),
        "copy-pipe"
        | "copy-pipe-no-clear"
        | "copy-pipe-and-cancel"
        | "copy-pipe-end-of-line"
        | "copy-pipe-end-of-line-and-cancel"
        | "copy-pipe-line"
        | "copy-pipe-line-and-cancel" => (0, 2),
        _ => (0, 0),
    };
    if positional.len() < minimum || positional.len() > maximum {
        return None;
    }
    Some((options, positional))
}

fn copy_end_of_line_action(action: CopyModeAction) -> CopyModeAction {
    let CopyModeAction::CopySelection(copy) = action else {
        unreachable!("copy helper always returns a selection action");
    };
    CopyModeAction::CopyEndOfLine(copy)
}

fn copy_line_action(action: CopyModeAction) -> CopyModeAction {
    let CopyModeAction::CopySelection(copy) = action else {
        unreachable!("copy helper always returns a selection action");
    };
    CopyModeAction::CopyLine(copy)
}

fn copy_selection_mode(argument: Option<&str>) -> Option<CopySelectionMode> {
    match argument {
        None => Some(CopySelectionMode::Char),
        Some(value) if value.eq_ignore_ascii_case("char") || value.eq_ignore_ascii_case("c") => {
            Some(CopySelectionMode::Char)
        }
        Some(value) if value.eq_ignore_ascii_case("word") || value.eq_ignore_ascii_case("w") => {
            Some(CopySelectionMode::Word)
        }
        Some(value) if value.eq_ignore_ascii_case("line") || value.eq_ignore_ascii_case("l") => {
            Some(CopySelectionMode::Line)
        }
        Some(_) => None,
    }
}

fn copy_goto_line_action(arguments: &[RawText]) -> Option<CopyModeAction> {
    let [line] = arguments else { return None };
    let line = pinned_strtonum(line, -1, i64::from(i32::MAX))
        .and_then(|parsed| i32::try_from(parsed).ok());
    Some(CopyModeAction::GotoLine {
        line,
        option_absolute: false,
    })
}

fn pinned_strtonum(value: &str, minimum: i64, maximum: i64) -> Option<i64> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == 0x0b)
    {
        index += 1;
    }
    let start = index;
    if matches!(bytes.get(index), Some(b'+' | b'-')) {
        index += 1;
    }
    let digits = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == digits || index != bytes.len() {
        return None;
    }
    let parsed = value.get(start..index)?.parse::<i64>().ok()?;
    (minimum..=maximum).contains(&parsed).then_some(parsed)
}

fn copy_pipe_action(
    command_name: &str,
    arguments: &[RawText],
    options: &Options,
    set_clipboard: SetClipboard,
    copy_command: &str,
    copy: bool,
) -> Result<CopyModeAction, ServerError> {
    let pipe = arguments.first().map_or(copy_command, RawText::as_str);
    if pipe.len() > MAX_COPY_COMMAND_BYTES {
        return Err(ServerError::InvalidCommand(format!(
            "copy pipe command exceeds {MAX_COPY_COMMAND_BYTES} bytes"
        )));
    }
    let prefix = if copy {
        arguments.get(1).map(ToString::to_string)
    } else {
        None
    };
    Ok(CopyModeAction::copy_selection(CopyModeCopy {
        request_id: 0,
        clipboard: copy && !options.has("-C") && set_clipboard != SetClipboard::Off,
        buffer: (copy && !options.has("-P")).then_some(PasteBufferAction::Create { prefix }),
        pipe: (!pipe.is_empty()).then(|| pipe.to_owned()),
        clear_selection: !command_name.ends_with("-no-clear"),
        cancel: command_name.ends_with("-and-cancel"),
    }))
}

fn copy_jump_action(
    arguments: &[RawText],
    direction: CopyJumpDirection,
    to: bool,
) -> Option<CopyModeAction> {
    if arguments.len() != 1 || arguments[0].is_empty() {
        return None;
    }
    Some(CopyModeAction::Jump(CopyJump {
        target: arguments[0].to_string(),
        direction,
        to,
    }))
}

fn copy_selection_action(
    arguments: &[RawText],
    options: &Options,
    set_clipboard: SetClipboard,
    clear_selection: bool,
    cancel: bool,
) -> CopyModeAction {
    CopyModeAction::copy_selection(CopyModeCopy {
        request_id: 0,
        clipboard: !options.has("-C") && set_clipboard != SetClipboard::Off,
        buffer: (!options.has("-P")).then(|| PasteBufferAction::Create {
            prefix: arguments.first().map(ToString::to_string),
        }),
        pipe: None,
        clear_selection,
        cancel,
    })
}

#[cfg(test)]
mod arguments_tests;

#[cfg(test)]
mod callbacks_tests;

#[cfg(test)]
mod environment_tests;

#[cfg(test)]
mod format_context_tests;

#[cfg(test)]
mod hooks_tests;

#[cfg(test)]
mod key_bindings_tests;

#[cfg(test)]
mod messages_and_prompts_tests;

#[cfg(test)]
mod modes_tests;

#[cfg(test)]
mod option_effects_tests;

#[cfg(test)]
mod option_formats_tests;

#[cfg(test)]
mod option_store_tests;

#[cfg(test)]
mod pane_spawn_tests;

#[cfg(test)]
mod panes_tests;

#[cfg(test)]
mod send_keys_tests;

#[cfg(test)]
mod sessions_tests;

#[cfg(test)]
mod set_option_tests;

#[cfg(test)]
mod show_options_tests;

#[cfg(test)]
mod targets_tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod windows_tests;
