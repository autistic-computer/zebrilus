use super::*;

macro_rules! variable {
    ($name:literal, $scope:ident, $backing:ident) => {
        FormatVariableSpec {
            name: $name,
            scope: FormatScope::$scope,
            kind: FormatKind::String,
            backing: FormatBacking::$backing,
            callback: |tree, format_type| {
                tree.resolve(FormatScope::$scope, FormatBacking::$backing, format_type)
            },
        }
    };
    ($name:literal, $scope:ident, Time, $backing:ident) => {
        FormatVariableSpec {
            name: $name,
            scope: FormatScope::$scope,
            kind: FormatKind::Time,
            backing: FormatBacking::$backing,
            callback: |tree, format_type| {
                tree.resolve(FormatScope::$scope, FormatBacking::$backing, format_type)
            },
        }
    };
}

pub(super) const FORMAT_VARIABLES: [FormatVariableSpec; 214] = [
    variable!("active_window_index", Session, ActiveWindowIndex),
    variable!("alternate_on", Terminal, StatusHook),
    variable!("alternate_saved_x", Terminal, Zero),
    variable!("alternate_saved_y", Terminal, Zero),
    variable!("bracket_paste_flag", Terminal, Zero),
    variable!("buffer_created", Buffer, Time, StatusHook),
    variable!("buffer_full", Buffer, StatusHook),
    variable!("buffer_mode_format", Buffer, Empty),
    variable!("buffer_name", Buffer, StatusHook),
    variable!("buffer_sample", Buffer, StatusHook),
    variable!("buffer_size", Buffer, StatusHook),
    variable!("client_activity", Client, Time, StatusHook),
    variable!("client_cell_height", Client, StatusHook),
    variable!("client_cell_width", Client, StatusHook),
    variable!("client_colours", Client, StatusHook),
    variable!("client_control_mode", Client, StatusHook),
    variable!("client_created", Client, Time, StatusHook),
    variable!("client_discarded", Client, StatusHook),
    variable!("client_flags", Client, StatusHook),
    variable!("client_height", Client, StatusHook),
    variable!("client_key_table", Client, StatusHook),
    variable!("client_last_session", Client, StatusHook),
    variable!("client_mode_format", Client, Empty),
    variable!("client_name", Client, StatusHook),
    variable!("client_pid", Client, StatusHook),
    variable!("client_prefix", Client, StatusHook),
    variable!("client_readonly", Client, StatusHook),
    variable!("client_session", Client, StatusHook),
    variable!("client_termfeatures", Client, StatusHook),
    variable!("client_termname", Client, StatusHook),
    variable!("client_termtype", Client, StatusHook),
    variable!("client_theme", Client, StatusHook),
    variable!("client_tty", Client, StatusHook),
    variable!("client_uid", Client, StatusHook),
    variable!("client_user", Client, StatusHook),
    variable!("client_utf8", Client, StatusHook),
    variable!("client_width", Client, StatusHook),
    variable!("client_written", Client, StatusHook),
    variable!("config_files", Server, ConfigFiles),
    variable!("cursor_blinking", Terminal, Zero),
    variable!("cursor_character", Terminal, Empty),
    variable!("cursor_colour", Terminal, Empty),
    variable!("cursor_flag", Terminal, StatusHook),
    variable!("cursor_shape", Terminal, Zero),
    variable!("cursor_very_visible", Terminal, Zero),
    variable!("cursor_x", Terminal, StatusHook),
    variable!("cursor_y", Terminal, StatusHook),
    variable!("history_added", Terminal, Zero),
    variable!("history_all_bytes", Terminal, Zero),
    variable!("history_bytes", Terminal, Zero),
    variable!("history_collected", Terminal, Zero),
    variable!("history_generation", Terminal, Zero),
    variable!("history_limit", Terminal, HistoryLimit),
    variable!("history_size", Terminal, StatusHook),
    variable!("host", Server, Host),
    variable!("host_short", Server, HostShort),
    variable!("insert_flag", Terminal, Zero),
    variable!("keypad_cursor_flag", Terminal, Zero),
    variable!("keypad_flag", Terminal, Zero),
    variable!("last_window_index", Session, LastWindowIndex),
    variable!("mouse_all_flag", Terminal, Zero),
    variable!("mouse_any_flag", Terminal, StatusHook),
    variable!("mouse_button_flag", Terminal, Zero),
    variable!("mouse_hyperlink", Terminal, StatusHook),
    variable!("mouse_line", Terminal, StatusHook),
    variable!("mouse_pane", Terminal, StatusHook),
    variable!("mouse_sgr_flag", Terminal, Zero),
    variable!("mouse_standard_flag", Terminal, Zero),
    variable!("mouse_status_line", Terminal, Empty),
    variable!("mouse_status_range", Terminal, Empty),
    variable!("mouse_utf8_flag", Terminal, Zero),
    variable!("mouse_word", Terminal, StatusHook),
    variable!("mouse_x", Terminal, StatusHook),
    variable!("mouse_y", Terminal, StatusHook),
    variable!("next_session_id", Server, NextSessionId),
    variable!("origin_flag", Terminal, Zero),
    variable!("pane_active", Pane, PaneActive),
    variable!("pane_at_bottom", Pane, PaneAtBottom),
    variable!("pane_at_left", Pane, PaneAtLeft),
    variable!("pane_at_right", Pane, PaneAtRight),
    variable!("pane_at_top", Pane, PaneAtTop),
    variable!("pane_bg", Pane, Empty),
    variable!("pane_bottom", Pane, PaneBottom),
    variable!("pane_command_duration", Pane, StatusHook),
    variable!("pane_command_end_time", Pane, Time, StatusHook),
    variable!("pane_command_running", Pane, StatusHook),
    variable!("pane_command_start_time", Pane, Time, StatusHook),
    variable!("pane_command_status", Pane, StatusHook),
    variable!("pane_current_command", Pane, PaneCurrentCommand),
    variable!("pane_current_path", Pane, PaneCurrentPath),
    variable!("pane_dead", Pane, PaneDead),
    variable!("pane_dead_signal", Pane, PaneDeadSignal),
    variable!("pane_dead_status", Pane, PaneDeadStatus),
    variable!("pane_dead_time", Pane, Time, PaneDeadTime),
    variable!("pane_fg", Pane, Empty),
    variable!("pane_flags", Pane, PaneFlags),
    variable!("pane_floating_flag", Pane, PaneFloatingFlag),
    variable!("pane_format", Pane, PaneFormat),
    variable!("pane_height", Pane, PaneHeight),
    variable!("pane_id", Pane, PaneId),
    variable!("pane_in_mode", Pane, StatusHook),
    variable!("pane_index", Pane, PaneIndex),
    variable!("pane_input_off", Pane, PaneInputOff),
    variable!("pane_key_mode", Pane, Empty),
    variable!("pane_last", Pane, PaneLast),
    variable!("pane_last_output_time", Pane, Time, StatusHook),
    variable!("pane_last_prompt_time", Pane, Time, StatusHook),
    variable!("pane_left", Pane, PaneLeft),
    variable!("pane_marked", Pane, PaneMarked),
    variable!("pane_marked_set", Pane, PaneMarkedSet),
    variable!("pane_modal_flag", Pane, PaneModalFlag),
    variable!("pane_mode", Pane, StatusHook),
    variable!("pane_output_generation", Pane, StatusHook),
    variable!("pane_path", Pane, PanePath),
    variable!("pane_pb_progress", Pane, StatusHook),
    variable!("pane_pb_state", Pane, StatusHook),
    variable!("pane_pid", Pane, PanePid),
    variable!("pane_pipe", Pane, StatusHook),
    variable!("pane_pipe_pid", Pane, StatusHook),
    variable!("pane_private_modes", Pane, StatusHook),
    variable!("pane_right", Pane, PaneRight),
    variable!("pane_search_string", Pane, StatusHook),
    variable!("pane_start_command", Pane, PaneStartCommand),
    variable!("pane_start_command_list", Pane, PaneStartCommandList),
    variable!("pane_start_path", Pane, PaneStartPath),
    variable!("pane_synchronized", Pane, PaneSynchronized),
    variable!("pane_tabs", Pane, Empty),
    variable!("pane_title", Pane, PaneTitle),
    variable!("pane_top", Pane, PaneTop),
    variable!("pane_tty", Pane, PaneTty),
    variable!("pane_unseen_changes", Pane, StatusHook),
    variable!("pane_unzoomed_height", Pane, PaneUnzoomedHeight),
    variable!("pane_unzoomed_width", Pane, PaneUnzoomedWidth),
    variable!("pane_width", Pane, PaneWidth),
    variable!("pane_x", Pane, PaneX),
    variable!("pane_y", Pane, PaneY),
    variable!("pane_z", Pane, PaneZ),
    variable!("pane_zoomed_flag", Pane, PaneZoomed),
    variable!("pid", Server, Pid),
    variable!("scroll_region_lower", Terminal, Zero),
    variable!("scroll_region_upper", Terminal, Zero),
    variable!("server_sessions", Server, ServerSessions),
    variable!("session_active", Session, SessionActive),
    variable!("session_activity", Session, Time, SessionActivity),
    variable!("session_activity_flag", Session, SessionActivityFlag),
    variable!("session_alert", Session, SessionAlert),
    variable!("session_alerts", Session, SessionAlerts),
    variable!("session_attached", Session, SessionAttached),
    variable!("session_attached_list", Session, SessionAttachedList),
    variable!("session_bell_flag", Session, SessionBell),
    variable!("session_created", Session, Time, SessionCreated),
    variable!("session_format", Session, SessionFormat),
    variable!("session_group", Session, Empty),
    variable!("session_group_attached", Session, Empty),
    variable!("session_group_attached_list", Session, Empty),
    variable!("session_group_list", Session, Empty),
    variable!("session_group_many_attached", Session, Empty),
    variable!("session_group_size", Session, Empty),
    variable!("session_grouped", Session, Zero),
    variable!("session_id", Session, SessionId),
    variable!("session_last_attached", Session, Time, StatusHook),
    variable!("session_many_attached", Session, SessionManyAttached),
    variable!("session_marked", Session, SessionMarked),
    variable!("session_name", Session, SessionName),
    variable!("session_path", Session, SessionPath),
    variable!("session_silence_flag", Session, SessionSilenceFlag),
    variable!("session_stack", Session, SessionStack),
    variable!("session_windows", Session, SessionWindows),
    variable!("sixel_support", Server, Zero),
    variable!("socket_path", Server, SocketPath),
    variable!("start_time", Server, Time, StartTime),
    variable!("synchronized_output_flag", Terminal, Zero),
    variable!("tree_mode_format", Server, Empty),
    variable!("uid", Server, Uid),
    variable!("user", Server, User),
    variable!("version", Server, Version),
    variable!("window_active", Window, WindowActive),
    variable!("window_active_clients", Window, WindowActiveClients),
    variable!(
        "window_active_clients_list",
        Window,
        WindowActiveClientsList
    ),
    variable!("window_active_sessions", Window, WindowActiveSessions),
    variable!(
        "window_active_sessions_list",
        Window,
        WindowActiveSessionsList
    ),
    variable!("window_activity", Window, Time, WindowActivity),
    variable!("window_activity_flag", Window, WindowActivityFlag),
    variable!("window_bell_flag", Window, WindowBell),
    variable!("window_bigger", Window, StatusHook),
    variable!("window_cell_height", Window, StatusHook),
    variable!("window_cell_width", Window, StatusHook),
    variable!("window_end_flag", Window, WindowEnd),
    variable!("window_flags", Window, WindowFlags),
    variable!("window_format", Window, WindowFormat),
    variable!("window_height", Window, WindowHeight),
    variable!("window_id", Window, WindowId),
    variable!("window_index", Window, WindowIndex),
    variable!("window_last_flag", Window, WindowLast),
    variable!("window_layout", Window, WindowLayout),
    variable!("window_linked", Window, WindowLinked),
    variable!("window_linked_sessions", Window, WindowLinkedSessions),
    variable!(
        "window_linked_sessions_list",
        Window,
        WindowLinkedSessionsList
    ),
    variable!("window_manual_height", Window, WindowManualHeight),
    variable!("window_manual_width", Window, WindowManualWidth),
    variable!("window_marked_flag", Window, WindowMarkedFlag),
    variable!("window_modal_pane", Window, WindowModalPane),
    variable!("window_name", Window, WindowName),
    variable!("window_offset_x", Window, StatusHook),
    variable!("window_offset_y", Window, StatusHook),
    variable!("window_panes", Window, WindowPanes),
    variable!("window_raw_flags", Window, WindowRawFlags),
    variable!("window_silence_flag", Window, WindowSilenceFlag),
    variable!("window_stack_index", Window, WindowStackIndex),
    variable!("window_start_flag", Window, WindowStart),
    variable!("window_visible_layout", Window, WindowVisibleLayout),
    variable!("window_width", Window, WindowWidth),
    variable!("window_zoomed_flag", Window, WindowZoomed),
    variable!("wrap_flag", Terminal, One),
];

pub(super) fn format_variable(name: &str) -> Option<&'static FormatVariableSpec> {
    FORMAT_VARIABLES
        .binary_search_by_key(&name, |variable| variable.name)
        .ok()
        .map(|index| &FORMAT_VARIABLES[index])
}

#[must_use]
pub fn format_variable_is_known(name: &str) -> bool {
    format_variable(name).is_some()
}

#[must_use]
pub fn format_variable_is_captured(name: &str) -> bool {
    format_variable(name).is_some_and(|spec| {
        !matches!(spec.backing, FormatBacking::StatusHook)
            && !matches!(spec.scope, FormatScope::Terminal)
    })
}

#[cfg(test)]
pub(crate) fn format_variable_names() -> impl Iterator<Item = &'static str> {
    FORMAT_VARIABLES.iter().map(|variable| variable.name)
}

#[cfg(test)]
const fn format_backing_is_tracked(backing: FormatBacking) -> bool {
    matches!(
        backing,
        FormatBacking::Empty
            | FormatBacking::Zero
            | FormatBacking::One
            | FormatBacking::WindowLinked
    )
}

#[cfg(test)]
pub(crate) fn constant_format_variable_names() -> impl Iterator<Item = &'static str> {
    FORMAT_VARIABLES
        .iter()
        .filter_map(|variable| format_backing_is_tracked(variable.backing).then_some(variable.name))
}

#[cfg(test)]
pub(crate) fn direct_format_variable_names() -> impl Iterator<Item = &'static str> {
    FORMAT_VARIABLES.iter().filter_map(|variable| {
        (!format_backing_is_tracked(variable.backing)
            && variable.backing != FormatBacking::StatusHook)
            .then_some(variable.name)
    })
}

#[doc(hidden)]
pub fn delegated_format_variable_names() -> impl Iterator<Item = &'static str> {
    FORMAT_VARIABLES.iter().filter_map(|variable| {
        (variable.backing == FormatBacking::StatusHook).then_some(variable.name)
    })
}
