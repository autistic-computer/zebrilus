use super::*;

pub(super) struct Stub;

impl StatusHooks for Stub {
    fn strftime(&mut self, literal: &str) -> String {
        literal.replace("%H:%M", "09:41").replace("%Y", "2026")
    }

    fn shell(&mut self, command: &str, _tag: &FormatJobTag) -> String {
        format!("<{command}>")
    }
}

pub(super) fn context() -> StatusContext<'static> {
    StatusContext::from(StatusValues {
        active_window_index: Some(1),
        config_files: "/tmp/first.conf,/tmp/second.conf".to_owned(),
        history_limit: Some(2_000),
        host: "tower.local".to_owned(),
        host_short: "tower".to_owned(),
        last_window_index: Some(3),
        next_session_id: "$5".to_owned(),
        pane_active: Some(true),
        pane_at_bottom: Some(true),
        pane_at_left: Some(false),
        pane_at_right: Some(true),
        pane_at_top: Some(true),
        pane_bottom: Some(49),
        pane_flags: "*".to_owned(),
        pane_height: Some(50),
        pane_id: "%7".to_owned(),
        pane_index: 0,
        pane_last: Some(false),
        pane_left: Some(80),
        pane_right: Some(159),
        pane_synchronized: false,
        pane_title: "/tmp/a b/main".to_owned(),
        pane_top: Some(0),
        pane_width: Some(80),
        pane_x: Some(80),
        pane_y: Some(0),
        pane_z: Some(1),
        pane_zoomed: false,
        pid: 42,
        server_sessions: 2,
        session_active: Some(true),
        session_activity: Some(1_700_000_000),
        session_attached: 1,
        session_attached_list: "client".to_owned(),
        session_id: "$4".to_owned(),
        session_name: "work".to_owned(),
        session_path: "/tmp/work".to_owned(),
        session_stack: "1,0".to_owned(),
        session_windows: 4,
        socket_path: "/tmp/tmux.sock".to_owned(),
        start_time: Some(946_728_000),
        uid: "1000".to_owned(),
        user: "fabrico".to_owned(),
        version: "0.2.0".to_owned(),
        window_active: Some(true),
        window_active_clients: 1,
        window_active_clients_list: "client".to_owned(),
        window_active_sessions: 1,
        window_active_sessions_list: "work".to_owned(),
        window_activity: Some(1_700_000_000),
        window_end: Some(false),
        window_height: Some(50),
        window_id: "@5".to_owned(),
        window_index: 1,
        window_last: Some(true),
        window_layout: "e582,160x50,0,0,7".to_owned(),
        window_linked_sessions: 1,
        window_linked_sessions_list: "work".to_owned(),
        window_name: "main".to_owned(),
        window_panes: 2,
        window_stack_index: 1,
        window_start: Some(false),
        window_visible_layout: "e582,160x50,0,0,7".to_owned(),
        window_width: Some(160),
        ..StatusValues::default()
    })
}

pub(super) fn expand(format: &str) -> String {
    expand_status(format, &context(), &mut Stub)
}
