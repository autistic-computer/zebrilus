mod arguments;
mod callbacks;
mod customize;
mod environment;
mod execution_context;
mod hooks;
mod key_bindings;
mod messages_and_prompts;
mod mode_prompt;
mod modes;
mod option_formats;
mod option_lookup;
mod option_store;
mod pane_spawn;
mod panes;
mod panes_mode;
mod runtime_facts;
mod send_keys;
mod sessions;
mod set_option;
mod show_options;
mod signals;
mod status_hook_adapters;
mod switch_mode;
mod windows;
use arguments::*;
pub(crate) use callbacks::format_key_command;
use callbacks::*;
pub use callbacks::{format_command, if_shell_truthy, validate_static_command_chain};
pub use customize::{
    CUSTOMIZE_MENU_ITEMS, CUSTOMIZE_OUTSIDE_MENU_ITEMS, CustomizeEdit, CustomizeMenu,
    CustomizeMenuItem, CustomizeMode, CustomizeResult, customize_menu_feed,
};
use environment::*;
pub use execution_context::resolve_invoking_mouse_targets;
use execution_context::*;
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
use runtime_facts::*;
pub(crate) use send_keys::copy_mode_probe_action;
pub use send_keys::{
    copy_mode_action_is_read_only_safe, send_keys_is_read_only_safe, send_keys_target_client,
};
use set_option::*;
use show_options::*;
pub use signals::tmux_signal_name;
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

struct ShownOptionHooks<'a, H> {
    inner: &'a mut H,
    values: &'a [(&'static str, String)],
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ClientTerminal {
    NoClient,
    Absent,
    #[default]
    Present,
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

#[cfg(test)]
mod arguments_tests;

#[cfg(test)]
mod callbacks_tests;

#[cfg(test)]
mod display_message_options_tests;

#[cfg(test)]
mod environment_tests;

#[cfg(test)]
mod format_cache_tests;

#[cfg(test)]
mod format_context_tests;

#[cfg(test)]
mod hooks_tests;

#[cfg(test)]
mod key_bindings_tests;

#[cfg(all(test, unix))]
mod key_listing_tests;

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
mod runtime_facts_tests;

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
