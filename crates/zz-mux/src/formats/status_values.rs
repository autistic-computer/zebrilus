use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusValues {
    pub active_window_index: Option<u32>,
    pub config_files: String,
    pub history_limit: Option<usize>,
    pub host: String,
    pub host_short: String,
    pub last_window_index: Option<u32>,
    pub next_session_id: String,
    pub pane_active: Option<bool>,
    pub pane_at_bottom: Option<bool>,
    pub pane_at_left: Option<bool>,
    pub pane_at_right: Option<bool>,
    pub pane_at_top: Option<bool>,
    pub pane_bottom: Option<i32>,
    pub pane_current_command: String,
    pub pane_current_path: String,
    pub pane_dead: Option<bool>,
    pub pane_dead_signal: String,
    pub pane_dead_status: Option<u32>,
    pub pane_dead_time: Option<i64>,
    pub pane_input_off: Option<bool>,
    pub pane_marked: Option<bool>,
    pub pane_marked_set: Option<bool>,
    pub pane_path: String,
    pub pane_flags: String,
    pub pane_height: Option<u16>,
    pub pane_id: String,
    pub pane_index: u32,
    pub pane_last: Option<bool>,
    pub pane_left: Option<i32>,
    pub pane_right: Option<i32>,
    pub pane_pid: Option<u32>,
    pub pane_start_command: String,
    pub pane_start_command_list: String,
    pub pane_start_path: String,
    pub pane_synchronized: bool,
    pub pane_title: String,
    pub pane_top: Option<i32>,
    pub pane_tty: String,
    pub pane_unzoomed_height: Option<u16>,
    pub pane_unzoomed_width: Option<u16>,
    pub pane_width: Option<u16>,
    pub pane_x: Option<i32>,
    pub pane_y: Option<i32>,
    pub pane_z: Option<usize>,
    pub pane_zoomed: bool,
    pub pane_floating: bool,
    pub pane_over_zoom: bool,
    pub pane_modal: bool,
    pub window_modal_pane: String,
    pub pid: u32,
    pub server_sessions: usize,
    pub session_active: Option<bool>,
    pub session_alert: String,
    pub session_alerts: String,
    pub session_attached: usize,
    pub session_attached_list: String,
    pub session_bell: bool,
    pub session_activity_flag: bool,
    pub session_silence_flag: bool,
    pub session_activity: Option<i64>,
    pub session_created: Option<i64>,
    pub session_id: String,
    pub session_many_attached: bool,
    pub session_marked: Option<bool>,
    pub session_name: String,
    pub session_path: String,
    pub session_stack: String,
    pub session_windows: usize,
    pub socket_path: String,
    pub start_time: Option<i64>,
    pub uid: String,
    pub user: String,
    pub version: String,
    pub window_active: Option<bool>,
    pub window_active_clients: usize,
    pub window_active_clients_list: String,
    pub window_active_sessions: usize,
    pub window_active_sessions_list: String,
    pub window_activity: Option<i64>,
    pub window_activity_alert: bool,
    pub window_bell: bool,
    pub window_end: Option<bool>,
    pub window_height: Option<u16>,
    pub window_id: String,
    pub window_index: u32,
    pub window_last: Option<bool>,
    pub window_layout: String,
    pub window_linked_sessions: usize,
    pub window_linked_sessions_list: String,
    pub window_manual_height: Option<u16>,
    pub window_manual_width: Option<u16>,
    pub window_marked_flag: Option<bool>,
    pub window_name: String,
    pub window_panes: usize,
    pub window_silence_alert: bool,
    pub window_stack_index: usize,
    pub window_start: Option<bool>,
    pub window_visible_layout: String,
    pub window_width: Option<u16>,
    pub window_zoomed: bool,
    #[doc(hidden)]
    pub format_now: Option<i64>,
    #[doc(hidden)]
    pub session_sort_activity: u64,
}

impl std::ops::Deref for StatusContext<'_> {
    type Target = StatusValues;

    fn deref(&self) -> &StatusValues {
        self.values.get_or_init(|| {
            let mut values = self
                .tree
                .as_ref()
                .map_or_else(StatusValues::default, |tree| {
                    tree.engine.build_status_values(
                        tree.session,
                        tree.window,
                        tree.pane,
                        tree.format_client,
                        &mut LayoutDumps::default(),
                    )
                });
            for (name, value) in self.variables.iter() {
                apply_context_value(&mut values, name, value);
            }
            Box::new(values)
        })
    }
}

impl std::ops::DerefMut for StatusContext<'_> {
    fn deref_mut(&mut self) -> &mut StatusValues {
        let _ = std::ops::Deref::deref(self);
        self.tree = None;
        Arc::make_mut(&mut self.variables).clear();
        self.capture_revision = None;
        self.values.get_mut().expect("materialized context")
    }
}

impl From<StatusValues> for StatusContext<'static> {
    fn from(values: StatusValues) -> Self {
        Self {
            session_id: values.session_id.clone(),
            window_id: values.window_id.clone(),
            pane_id: values.pane_id.clone(),
            format_now: values.format_now,
            values: OnceLock::from(Box::new(values)),
            tree: None,
            variables: Arc::default(),
            format_client: FormatClient::NoClient,
            capture_revision: None,
            format_universe: FormatUniverseRef::default(),
        }
    }
}

fn apply_context_value(values: &mut StatusValues, name: &str, value: &str) {
    let Some(spec) = format_variable(name) else {
        return;
    };
    match spec.backing {
        FormatBacking::ActiveWindowIndex => values.active_window_index = value.parse().ok(),
        FormatBacking::ConfigFiles => value.clone_into(&mut values.config_files),
        FormatBacking::HistoryLimit => values.history_limit = value.parse().ok(),
        FormatBacking::Host => value.clone_into(&mut values.host),
        FormatBacking::HostShort => value.clone_into(&mut values.host_short),
        FormatBacking::LastWindowIndex => values.last_window_index = value.parse().ok(),
        FormatBacking::NextSessionId => value.clone_into(&mut values.next_session_id),
        FormatBacking::PaneActive => {
            values.pane_active = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::PaneAtBottom => {
            values.pane_at_bottom = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::PaneAtLeft => {
            values.pane_at_left = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::PaneAtRight => {
            values.pane_at_right = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::PaneAtTop => {
            values.pane_at_top = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::PaneBottom => values.pane_bottom = value.parse().ok(),
        FormatBacking::PaneCurrentCommand => value.clone_into(&mut values.pane_current_command),
        FormatBacking::PaneCurrentPath => value.clone_into(&mut values.pane_current_path),
        FormatBacking::PaneDead => values.pane_dead = (!value.is_empty()).then_some(value == "1"),
        FormatBacking::PaneDeadSignal => value.clone_into(&mut values.pane_dead_signal),
        FormatBacking::PaneDeadStatus => values.pane_dead_status = value.parse().ok(),
        FormatBacking::PaneDeadTime => values.pane_dead_time = value.parse().ok(),
        FormatBacking::PaneInputOff => {
            values.pane_input_off = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::PaneMarked => {
            values.pane_marked = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::PaneMarkedSet => {
            values.pane_marked_set = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::PanePath => value.clone_into(&mut values.pane_path),
        FormatBacking::PaneFlags => value.clone_into(&mut values.pane_flags),
        FormatBacking::PaneHeight => values.pane_height = value.parse().ok(),
        FormatBacking::PaneId => value.clone_into(&mut values.pane_id),
        FormatBacking::PaneIndex => values.pane_index = value.parse().unwrap_or_default(),
        FormatBacking::PaneLast => values.pane_last = (!value.is_empty()).then_some(value == "1"),
        FormatBacking::PaneLeft => values.pane_left = value.parse().ok(),
        FormatBacking::PaneRight => values.pane_right = value.parse().ok(),
        FormatBacking::PanePid => values.pane_pid = value.parse().ok(),
        FormatBacking::PaneStartCommand => value.clone_into(&mut values.pane_start_command),
        FormatBacking::PaneStartCommandList => {
            value.clone_into(&mut values.pane_start_command_list);
        }
        FormatBacking::PaneStartPath => value.clone_into(&mut values.pane_start_path),
        FormatBacking::PaneSynchronized => values.pane_synchronized = value == "1",
        FormatBacking::PaneTitle => value.clone_into(&mut values.pane_title),
        FormatBacking::PaneTop => values.pane_top = value.parse().ok(),
        FormatBacking::PaneTty => value.clone_into(&mut values.pane_tty),
        FormatBacking::PaneUnzoomedHeight => values.pane_unzoomed_height = value.parse().ok(),
        FormatBacking::PaneUnzoomedWidth => values.pane_unzoomed_width = value.parse().ok(),
        FormatBacking::PaneWidth => values.pane_width = value.parse().ok(),
        FormatBacking::PaneX => values.pane_x = value.parse().ok(),
        FormatBacking::PaneY => values.pane_y = value.parse().ok(),
        FormatBacking::PaneZ => values.pane_z = value.parse().ok(),
        FormatBacking::PaneFloatingFlag => values.pane_floating = value == "1",
        FormatBacking::PaneModalFlag => values.pane_modal = value == "1",
        FormatBacking::WindowModalPane => value.clone_into(&mut values.window_modal_pane),
        FormatBacking::PaneZoomed => values.pane_zoomed = value == "1",
        FormatBacking::Pid => values.pid = value.parse().unwrap_or_default(),
        FormatBacking::ServerSessions => values.server_sessions = value.parse().unwrap_or_default(),
        FormatBacking::SessionAlert => value.clone_into(&mut values.session_alert),
        FormatBacking::SessionAlerts => value.clone_into(&mut values.session_alerts),
        FormatBacking::SessionAttached => {
            values.session_attached = value.parse().unwrap_or_default();
        }
        FormatBacking::SessionAttachedList => value.clone_into(&mut values.session_attached_list),
        FormatBacking::SessionBell => values.session_bell = value == "1",
        FormatBacking::SessionActivityFlag => values.session_activity_flag = value == "1",
        FormatBacking::SessionSilenceFlag => values.session_silence_flag = value == "1",
        FormatBacking::SessionActivity => values.session_activity = value.parse().ok(),
        FormatBacking::SessionActive => {
            values.session_active = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::SessionCreated => values.session_created = value.parse().ok(),
        FormatBacking::SessionId => value.clone_into(&mut values.session_id),
        FormatBacking::SessionManyAttached => values.session_many_attached = value == "1",
        FormatBacking::SessionMarked => {
            values.session_marked = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::SessionName => value.clone_into(&mut values.session_name),
        FormatBacking::SessionPath => value.clone_into(&mut values.session_path),
        FormatBacking::SessionStack => value.clone_into(&mut values.session_stack),
        FormatBacking::SessionWindows => values.session_windows = value.parse().unwrap_or_default(),
        FormatBacking::SocketPath => value.clone_into(&mut values.socket_path),
        FormatBacking::StartTime => values.start_time = value.parse().ok(),
        FormatBacking::Uid => value.clone_into(&mut values.uid),
        FormatBacking::User => value.clone_into(&mut values.user),
        FormatBacking::Version => value.clone_into(&mut values.version),
        FormatBacking::WindowActive => {
            values.window_active = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::WindowActiveClients => {
            values.window_active_clients = value.parse().unwrap_or_default();
        }
        FormatBacking::WindowActiveClientsList => {
            value.clone_into(&mut values.window_active_clients_list);
        }
        FormatBacking::WindowActiveSessions => {
            values.window_active_sessions = value.parse().unwrap_or_default();
        }
        FormatBacking::WindowActiveSessionsList => {
            value.clone_into(&mut values.window_active_sessions_list);
        }
        FormatBacking::WindowActivity => values.window_activity = value.parse().ok(),
        FormatBacking::WindowActivityFlag => values.window_activity_alert = value == "1",
        FormatBacking::WindowBell => values.window_bell = value == "1",
        FormatBacking::WindowEnd => values.window_end = (!value.is_empty()).then_some(value == "1"),
        FormatBacking::WindowHeight => values.window_height = value.parse().ok(),
        FormatBacking::WindowId => value.clone_into(&mut values.window_id),
        FormatBacking::WindowIndex => values.window_index = value.parse().unwrap_or_default(),
        FormatBacking::WindowLast => {
            values.window_last = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::WindowLayout => value.clone_into(&mut values.window_layout),
        FormatBacking::WindowLinkedSessions => {
            values.window_linked_sessions = value.parse().unwrap_or_default();
        }
        FormatBacking::WindowLinkedSessionsList => {
            value.clone_into(&mut values.window_linked_sessions_list);
        }
        FormatBacking::WindowManualHeight => values.window_manual_height = value.parse().ok(),
        FormatBacking::WindowManualWidth => values.window_manual_width = value.parse().ok(),
        FormatBacking::WindowMarkedFlag => {
            values.window_marked_flag = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::WindowName => value.clone_into(&mut values.window_name),
        FormatBacking::WindowPanes => values.window_panes = value.parse().unwrap_or_default(),
        FormatBacking::WindowStackIndex => {
            values.window_stack_index = value.parse().unwrap_or_default();
        }
        FormatBacking::WindowStart => {
            values.window_start = (!value.is_empty()).then_some(value == "1");
        }
        FormatBacking::WindowVisibleLayout => value.clone_into(&mut values.window_visible_layout),
        FormatBacking::WindowWidth => values.window_width = value.parse().ok(),
        FormatBacking::WindowZoomed => values.window_zoomed = value == "1",
        _ => {}
    }
}

impl StatusValues {
    #[must_use]
    pub fn variable(&self, name: &str) -> Option<Cow<'_, str>> {
        let spec = format_variable(name)?;
        Some(self.resolve(spec, FormatType::None))
    }

    fn resolve(&self, spec: &FormatVariableSpec, format_type: FormatType) -> Cow<'_, str> {
        let available = match spec.scope {
            FormatScope::Server | FormatScope::Buffer | FormatScope::Client => true,
            FormatScope::Session => {
                !self.session_id.is_empty()
                    || !self.session_name.is_empty()
                    || self.active_window_index.is_some()
            }
            FormatScope::Window => {
                !self.window_id.is_empty()
                    || !self.window_name.is_empty()
                    || self.window_width.is_some()
            }
            FormatScope::Pane | FormatScope::Terminal => {
                !self.pane_id.is_empty() || !self.pane_title.is_empty() || self.pane_width.is_some()
            }
        };
        if !available {
            return Cow::Borrowed("");
        }
        match spec.backing {
            FormatBacking::Empty | FormatBacking::StatusHook => Cow::Borrowed(""),
            FormatBacking::Zero => Cow::Borrowed("0"),
            FormatBacking::One => Cow::Borrowed("1"),
            FormatBacking::ActiveWindowIndex => optional_display(self.active_window_index),
            FormatBacking::ConfigFiles => Cow::Borrowed(self.config_files.as_str()),
            FormatBacking::HistoryLimit => optional_display(self.history_limit),
            FormatBacking::Host => Cow::Borrowed(self.host.as_str()),
            FormatBacking::HostShort => Cow::Borrowed(self.host_short.as_str()),
            FormatBacking::LastWindowIndex => optional_display(self.last_window_index),
            FormatBacking::NextSessionId => Cow::Borrowed(self.next_session_id.as_str()),
            FormatBacking::PaneActive => optional_bool(self.pane_active),
            FormatBacking::PaneAtBottom => optional_bool(self.pane_at_bottom),
            FormatBacking::PaneAtLeft => optional_bool(self.pane_at_left),
            FormatBacking::PaneAtRight => optional_bool(self.pane_at_right),
            FormatBacking::PaneAtTop => optional_bool(self.pane_at_top),
            FormatBacking::PaneBottom => optional_display(self.pane_bottom),
            FormatBacking::PaneCurrentCommand => Cow::Borrowed(self.pane_current_command.as_str()),
            FormatBacking::PaneCurrentPath => Cow::Borrowed(self.pane_current_path.as_str()),
            FormatBacking::PaneDead => optional_bool(self.pane_dead),
            FormatBacking::PaneDeadSignal => Cow::Borrowed(if self.pane_dead == Some(true) {
                self.pane_dead_signal.as_str()
            } else {
                ""
            }),
            FormatBacking::PaneDeadStatus => optional_display(self.pane_dead_status),
            FormatBacking::PaneDeadTime => optional_display(self.pane_dead_time),
            FormatBacking::PaneInputOff => optional_bool(self.pane_input_off),
            FormatBacking::PaneMarked => optional_bool(self.pane_marked),
            FormatBacking::PaneMarkedSet => optional_bool(self.pane_marked_set),
            FormatBacking::PanePath => Cow::Borrowed(self.pane_path.as_str()),
            FormatBacking::PaneFlags => Cow::Borrowed(self.pane_flags.as_str()),
            FormatBacking::PaneFormat => {
                Cow::Borrowed(bool_string(format_type == FormatType::Pane))
            }
            FormatBacking::PaneHeight => optional_display(self.pane_height),
            FormatBacking::PaneId => Cow::Borrowed(self.pane_id.as_str()),
            FormatBacking::PaneIndex => Cow::Owned(self.pane_index.to_string()),
            FormatBacking::PaneLast => optional_bool(self.pane_last),
            FormatBacking::PaneLeft => optional_display(self.pane_left),
            FormatBacking::PaneRight => optional_display(self.pane_right),
            FormatBacking::PanePid => optional_display(self.pane_pid),
            FormatBacking::PaneStartCommand => Cow::Borrowed(self.pane_start_command.as_str()),
            FormatBacking::PaneStartCommandList => {
                Cow::Borrowed(self.pane_start_command_list.as_str())
            }
            FormatBacking::PaneStartPath => Cow::Borrowed(self.pane_start_path.as_str()),
            FormatBacking::PaneSynchronized => Cow::Borrowed(bool_string(self.pane_synchronized)),
            FormatBacking::PaneTitle => Cow::Borrowed(self.pane_title.as_str()),
            FormatBacking::PaneTop => optional_display(self.pane_top),
            FormatBacking::PaneTty => Cow::Borrowed(self.pane_tty.as_str()),
            FormatBacking::PaneUnzoomedHeight => optional_display(self.pane_unzoomed_height),
            FormatBacking::PaneUnzoomedWidth => optional_display(self.pane_unzoomed_width),
            FormatBacking::PaneWidth => optional_display(self.pane_width),
            FormatBacking::PaneX => optional_display(self.pane_x),
            FormatBacking::PaneY => optional_display(self.pane_y),
            FormatBacking::PaneZ => optional_display(self.pane_z),
            FormatBacking::PaneFloatingFlag => Cow::Borrowed(bool_string(self.pane_floating)),
            FormatBacking::PaneModalFlag => Cow::Borrowed(bool_string(self.pane_modal)),
            FormatBacking::WindowModalPane => Cow::Borrowed(self.window_modal_pane.as_str()),
            FormatBacking::PaneZoomed => Cow::Borrowed(bool_string(self.pane_zoomed)),
            FormatBacking::Pid => Cow::Owned(self.pid.to_string()),
            FormatBacking::ServerSessions => Cow::Owned(self.server_sessions.to_string()),
            FormatBacking::SessionAlert => Cow::Borrowed(self.session_alert.as_str()),
            FormatBacking::SessionAlerts => Cow::Borrowed(self.session_alerts.as_str()),
            FormatBacking::SessionAttached => Cow::Owned(self.session_attached.to_string()),
            FormatBacking::SessionAttachedList => {
                Cow::Borrowed(self.session_attached_list.as_str())
            }
            FormatBacking::SessionBell => Cow::Borrowed(bool_string(self.session_bell)),
            FormatBacking::SessionActivityFlag => {
                Cow::Borrowed(bool_string(self.session_activity_flag))
            }
            FormatBacking::SessionSilenceFlag => {
                Cow::Borrowed(bool_string(self.session_silence_flag))
            }
            FormatBacking::SessionActivity => optional_display(self.session_activity),
            FormatBacking::SessionActive => optional_bool(self.session_active),
            FormatBacking::SessionCreated => optional_display(self.session_created),
            FormatBacking::SessionFormat => {
                Cow::Borrowed(bool_string(format_type == FormatType::Session))
            }
            FormatBacking::SessionId => Cow::Borrowed(self.session_id.as_str()),
            FormatBacking::SessionMarked => optional_bool(self.session_marked),
            FormatBacking::SessionManyAttached => {
                Cow::Borrowed(bool_string(self.session_many_attached))
            }
            FormatBacking::SessionName => Cow::Borrowed(self.session_name.as_str()),
            FormatBacking::SessionPath => Cow::Borrowed(self.session_path.as_str()),
            FormatBacking::SessionStack => Cow::Borrowed(self.session_stack.as_str()),
            FormatBacking::SessionWindows => Cow::Owned(self.session_windows.to_string()),
            FormatBacking::SocketPath => Cow::Borrowed(self.socket_path.as_str()),
            FormatBacking::StartTime => optional_display(self.start_time),
            FormatBacking::Uid => Cow::Borrowed(self.uid.as_str()),
            FormatBacking::User => Cow::Borrowed(self.user.as_str()),
            FormatBacking::Version => Cow::Borrowed(self.version.as_str()),
            FormatBacking::WindowActive => optional_bool(self.window_active),
            FormatBacking::WindowActiveClients => {
                Cow::Owned(self.window_active_clients.to_string())
            }
            FormatBacking::WindowActiveClientsList => {
                Cow::Borrowed(self.window_active_clients_list.as_str())
            }
            FormatBacking::WindowActiveSessions => {
                Cow::Owned(self.window_active_sessions.to_string())
            }
            FormatBacking::WindowActiveSessionsList => {
                Cow::Borrowed(self.window_active_sessions_list.as_str())
            }
            FormatBacking::WindowActivity => optional_display(self.window_activity),
            FormatBacking::WindowActivityFlag => {
                Cow::Borrowed(bool_string(self.window_activity_alert))
            }
            FormatBacking::WindowBell => Cow::Borrowed(bool_string(self.window_bell)),
            FormatBacking::WindowSilenceFlag => {
                Cow::Borrowed(bool_string(self.window_silence_alert))
            }
            FormatBacking::WindowEnd => optional_bool(self.window_end),
            FormatBacking::WindowFlags => Cow::Owned(self.window_flags(true)),
            FormatBacking::WindowFormat => {
                Cow::Borrowed(bool_string(format_type == FormatType::Window))
            }
            FormatBacking::WindowHeight => optional_display(self.window_height),
            FormatBacking::WindowId => Cow::Borrowed(self.window_id.as_str()),
            FormatBacking::WindowIndex => Cow::Owned(self.window_index.to_string()),
            FormatBacking::WindowLast => optional_bool(self.window_last),
            FormatBacking::WindowLayout => Cow::Borrowed(self.window_layout.as_str()),
            FormatBacking::WindowLinked => Cow::Borrowed(bool_string(false)),
            FormatBacking::WindowLinkedSessions => {
                Cow::Owned(self.window_linked_sessions.to_string())
            }
            FormatBacking::WindowLinkedSessionsList => {
                Cow::Borrowed(self.window_linked_sessions_list.as_str())
            }
            FormatBacking::WindowManualHeight => optional_display(self.window_manual_height),
            FormatBacking::WindowManualWidth => optional_display(self.window_manual_width),
            FormatBacking::WindowMarkedFlag => optional_bool(self.window_marked_flag),
            FormatBacking::WindowName => Cow::Borrowed(self.window_name.as_str()),
            FormatBacking::WindowPanes => Cow::Owned(self.window_panes.to_string()),
            FormatBacking::WindowRawFlags => Cow::Owned(self.window_flags(false)),
            FormatBacking::WindowStackIndex => Cow::Owned(self.window_stack_index.to_string()),
            FormatBacking::WindowStart => optional_bool(self.window_start),
            FormatBacking::WindowVisibleLayout => {
                Cow::Borrowed(self.window_visible_layout.as_str())
            }
            FormatBacking::WindowWidth => optional_display(self.window_width),
            FormatBacking::WindowZoomed => Cow::Borrowed(bool_string(self.window_zoomed)),
        }
    }

    fn window_flags(&self, escape: bool) -> String {
        let mut flags = String::new();
        if self.window_activity_alert {
            flags.push('#');
        }
        if self.window_bell {
            flags.push('!');
        }
        if self.window_silence_alert {
            flags.push('~');
        }
        if self.window_active.unwrap_or(false) {
            flags.push('*');
        }
        if self.window_last.unwrap_or(false) {
            flags.push('-');
        }
        if self.window_marked_flag.unwrap_or(false) {
            flags.push('M');
        }
        if !self.window_modal_pane.is_empty() {
            flags.push('O');
        }
        if self.window_zoomed {
            flags.push('Z');
        }
        if escape {
            flags.replace('#', "##")
        } else {
            flags
        }
    }
}

impl FormatVariables for StatusContext<'_> {
    fn variable(&self, name: &str) -> Option<Cow<'_, str>> {
        self.variable(name)
    }

    fn variable_kind(&self, name: &str) -> Option<FormatKind> {
        format_variable(name).map(|spec| spec.kind)
    }

    fn values(&self) -> &StatusContext<'_> {
        self
    }

    fn format_type(&self) -> FormatType {
        FormatType::None
    }
}

impl StatusContext<'_> {
    #[must_use]
    pub fn engine(&self) -> Option<&MuxEngine> {
        self.tree
            .as_ref()
            .map(|tree| tree.engine)
            .or(self.format_universe.engine)
    }
    #[must_use]
    pub fn variable(&self, name: &str) -> Option<Cow<'_, str>> {
        format_variable(name).map_or_else(
            || {
                self.variables
                    .get(name)
                    .map(|value| Cow::Borrowed(value.as_str()))
            },
            |spec| Some(self.resolve(spec, FormatType::None)),
        )
    }

    pub(super) fn resolve(
        &self,
        spec: &FormatVariableSpec,
        format_type: FormatType,
    ) -> Cow<'_, str> {
        let scope_id = match spec.backing {
            FormatBacking::SessionFormat => Some((&self.session_id, FormatType::Session)),
            FormatBacking::WindowFormat => Some((&self.window_id, FormatType::Window)),
            FormatBacking::PaneFormat => Some((&self.pane_id, FormatType::Pane)),
            _ => None,
        };
        if let Some((id, expected)) = scope_id {
            let available = !id.is_empty()
                || self.values.get().is_some_and(|values| match expected {
                    FormatType::Session => {
                        !values.session_name.is_empty() || values.active_window_index.is_some()
                    }
                    FormatType::Window => {
                        !values.window_name.is_empty() || values.window_width.is_some()
                    }
                    FormatType::Pane => {
                        !values.pane_title.is_empty() || values.pane_width.is_some()
                    }
                    FormatType::None => false,
                });
            return Cow::Borrowed(if available {
                bool_string(format_type == expected)
            } else {
                ""
            });
        }
        match spec.backing {
            FormatBacking::SessionId => return Cow::Borrowed(&self.session_id),
            FormatBacking::WindowId => return Cow::Borrowed(&self.window_id),
            FormatBacking::PaneId => return Cow::Borrowed(&self.pane_id),
            _ => {}
        }
        if let Some(value) = self.variables.get(spec.name) {
            return Cow::Borrowed(value);
        }
        self.tree.as_ref().map_or_else(
            || (**self).resolve(spec, format_type),
            |tree| (spec.callback)(tree, format_type),
        )
    }

    pub fn set_format_value(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        match name.as_str() {
            "session_id" => self.session_id.clone_from(&value),
            "window_id" => self.window_id.clone_from(&value),
            "pane_id" => self.pane_id.clone_from(&value),
            _ => {}
        }
        if let Some(values) = self.values.get_mut() {
            apply_context_value(values, &name, &value);
        }
        Arc::make_mut(&mut self.variables).insert(name, value);
    }

    #[must_use]
    pub fn detach(self, needs: FormatNeeds) -> StatusContext<'static> {
        if let Some(context) = self.cached_detached(needs, None, None) {
            return context;
        }
        self.detach_selected(needs, None, None, None)
    }

    #[must_use]
    pub fn detach_with_templates<'t>(
        self,
        needs: FormatNeeds,
        templates: impl IntoIterator<Item = &'t str>,
    ) -> StatusContext<'static> {
        let references = if let Some(engine) = self.tree.as_ref().map(|tree| tree.engine) {
            engine.cached_format_references_for_templates(templates)
        } else {
            Arc::new(
                templates
                    .into_iter()
                    .flat_map(|template| format_references(template).as_ref().to_owned())
                    .collect::<BTreeSet<_>>(),
            )
        };
        self.detach_with_shared_references(needs, &references)
    }

    #[must_use]
    pub fn detach_with_references(
        self,
        needs: FormatNeeds,
        references: &BTreeSet<String>,
    ) -> StatusContext<'static> {
        if let Some(context) = self.cached_detached(needs, Some(references), None) {
            return context;
        }
        self.detach_selected(
            needs,
            (!references.contains("*")).then_some(references),
            Some(references),
            None,
        )
    }

    #[must_use]
    pub fn detach_with_shared_references(
        self,
        needs: FormatNeeds,
        references: &Arc<BTreeSet<String>>,
    ) -> StatusContext<'static> {
        if let Some(context) = self.cached_detached(needs, Some(references), Some(references)) {
            return context;
        }
        self.detach_selected(
            needs,
            (!references.contains("*")).then_some(references),
            Some(references),
            Some(references),
        )
    }

    fn cached_detached(
        &self,
        needs: FormatNeeds,
        references: Option<&BTreeSet<String>>,
        shared: Option<&Arc<BTreeSet<String>>>,
    ) -> Option<StatusContext<'static>> {
        let tree = self.tree.as_ref()?;
        if !tree::borrowed_formats() || self.values.get().is_some() {
            return None;
        }
        let mut cache = tree.engine.format_context_cache.lock();
        let cached = cache.as_mut()?;
        let revision = FormatCaptureRevision::new(tree.engine);
        (cached.needs == needs
            && cached.matches_references(references, shared)
            && cached.target == (tree.session, tree.window, tree.pane)
            && cached.input_variables == self.variables
            && cached
                .context
                .capture_revision
                .as_deref()
                .is_some_and(|captured| captured.same_data(&revision))
            && cached.context.format_client == self.format_client
            && cached.context.session_id == self.session_id
            && cached.context.window_id == self.window_id
            && cached.context.pane_id == self.pane_id)
            .then(|| {
                cached.adopt_reference_identity(shared);
                cached.context.with_capture_clock(revision, self.format_now)
            })
    }

    pub(super) fn with_capture_clock(
        &self,
        revision: FormatCaptureRevision,
        now: Option<i64>,
    ) -> Self {
        let mut context = self.clone();
        context.format_now = now;
        if let Some(values) = context.values.get_mut() {
            values.format_now = now;
        }
        if context.capture_revision.as_deref() != Some(&revision) {
            context.capture_revision = Some(Arc::new(revision));
        }
        context
    }

    fn capture_variables(
        &self,
        specs: impl IntoIterator<Item = &'static FormatVariableSpec>,
        preserve_overrides: bool,
    ) -> BTreeMap<String, String> {
        if self.tree.is_none() {
            return BTreeMap::new();
        }
        specs
            .into_iter()
            .filter(|spec| !preserve_overrides || !self.variables.contains_key(spec.name))
            .map(|spec| {
                (
                    spec.name.to_owned(),
                    self.resolve(spec, FormatType::None).into_owned(),
                )
            })
            .collect()
    }

    fn detach_selected(
        self,
        needs: FormatNeeds,
        references: Option<&BTreeSet<String>>,
        cache_references: Option<&BTreeSet<String>>,
        shared: Option<&Arc<BTreeSet<String>>>,
    ) -> StatusContext<'static> {
        let engine = self.tree.as_ref().map(|tree| tree.engine);
        let revision = engine.map(|engine| Arc::new(FormatCaptureRevision::new(engine)));
        let cache_key = self
            .tree
            .as_ref()
            .filter(|_| tree::borrowed_formats() && self.values.get().is_none())
            .map(|tree| {
                (
                    tree.engine,
                    (tree.session, tree.window, tree.pane),
                    Arc::clone(&self.variables),
                )
            });
        let specs = references.map(|names| {
            names
                .iter()
                .filter_map(|name| format_variable(name))
                .collect::<Vec<_>>()
        });
        let format_universe = self.format_universe.detach(
            needs,
            &self.session_id,
            &self.window_id,
            &self.pane_id,
            specs.as_deref(),
        );
        let captured = specs.as_ref().map_or_else(
            || self.capture_variables(FORMAT_VARIABLES.iter(), true),
            |specs| self.capture_variables(specs.iter().copied(), true),
        );
        let mut variables = self.variables;
        if !captured.is_empty() {
            Arc::make_mut(&mut variables).extend(captured);
        }
        let context = StatusContext {
            values: self.values,
            tree: None,
            variables,
            format_client: self.format_client,
            capture_revision: revision.or(self.capture_revision),
            session_id: self.session_id,
            window_id: self.window_id,
            pane_id: self.pane_id,
            format_now: self.format_now,
            format_universe,
        };
        if let Some((engine, target, input_variables)) = cache_key {
            let bytes = context
                .retained_bytes()
                .saturating_add(variable_bytes(&input_variables))
                .saturating_add(cache_references.map_or(0, reference_bytes))
                .saturating_add(std::mem::size_of::<Option<Weak<BTreeSet<String>>>>())
                .saturating_add(
                    std::mem::size_of::<BTreeSet<String>>()
                        .saturating_add(std::mem::size_of::<usize>().saturating_mul(2)),
                );
            let mut cache = engine.format_context_cache.lock();
            *cache = (bytes <= FORMAT_CAPTURE_CACHE_BYTES).then(|| DetachedContextCache {
                target,
                input_variables,
                needs,
                references: cache_references.cloned(),
                context: context.clone(),
                reference_identity: shared.map(Arc::downgrade),
            });
        }
        context
    }

    pub(super) fn detach_values(
        &self,
        specs: Option<&[&'static FormatVariableSpec]>,
    ) -> StatusContext<'static> {
        let captured = specs.map_or_else(
            || self.capture_variables(FORMAT_VARIABLES.iter(), false),
            |specs| self.capture_variables(specs.iter().copied(), false),
        );
        let mut variables = Arc::clone(&self.variables);
        if !captured.is_empty() {
            Arc::make_mut(&mut variables).extend(captured);
        }
        StatusContext {
            values: self.values.clone(),
            tree: None,
            variables,
            format_client: self.format_client,
            capture_revision: self.capture_revision.clone(),
            session_id: self.session_id.clone(),
            window_id: self.window_id.clone(),
            pane_id: self.pane_id.clone(),
            format_now: self.format_now,
            format_universe: FormatUniverseRef::default(),
        }
    }

    #[must_use]
    pub fn has_captured_environment(&self) -> bool {
        self.capture_revision.is_some()
            && self.tree.is_none()
            && self.format_universe.engine.is_none()
            && self
                .format_universe
                .parts
                .built()
                .contains(FormatNeeds::ENVIRONMENT)
    }

    #[must_use]
    pub fn same_detached(&self, other: &StatusContext<'_>) -> bool {
        self.tree.is_none()
            && other.tree.is_none()
            && self.format_universe.engine.is_none()
            && other.format_universe.engine.is_none()
            && self.capture_revision == other.capture_revision
            && self.format_client == other.format_client
            && self.session_id == other.session_id
            && self.window_id == other.window_id
            && self.pane_id == other.pane_id
            && self.format_now == other.format_now
            && self.values.get() == other.values.get()
            && (Arc::ptr_eq(&self.variables, &other.variables) || self.variables == other.variables)
            && (Arc::ptr_eq(&self.format_universe.parts, &other.format_universe.parts)
                || self
                    .format_universe
                    .parts
                    .same_owned(&other.format_universe.parts))
    }

    #[must_use]
    pub fn same_detached_data(&self, other: &StatusContext<'_>) -> bool {
        self.tree.is_none()
            && other.tree.is_none()
            && self.format_universe.engine.is_none()
            && other.format_universe.engine.is_none()
            && self.values.get().is_none()
            && other.values.get().is_none()
            && self
                .capture_revision
                .as_deref()
                .zip(other.capture_revision.as_deref())
                .is_some_and(|(left, right)| left.same_data(right))
            && self.format_client == other.format_client
            && self.session_id == other.session_id
            && self.window_id == other.window_id
            && self.pane_id == other.pane_id
            && Arc::ptr_eq(&self.variables, &other.variables)
            && Arc::ptr_eq(&self.format_universe.parts, &other.format_universe.parts)
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        if self.values.get().is_some() {
            return FORMAT_CAPTURE_CACHE_BYTES + 1;
        }
        std::mem::size_of::<Self>()
            .saturating_add(self.session_id.capacity())
            .saturating_add(self.window_id.capacity())
            .saturating_add(self.pane_id.capacity())
            .saturating_add(variable_bytes(&self.variables))
            .saturating_add(self.format_universe.parts.retained_bytes())
    }

    #[must_use]
    pub fn format_universe_covers(&self, needs: FormatNeeds) -> bool {
        self.format_universe.engine.is_some() || self.format_universe.parts.built().contains(needs)
    }

    #[must_use]
    pub fn format_loops_available(&self, needs: FormatNeeds) -> bool {
        let windows = needs.contains(FormatNeeds::WINDOWS);
        let panes = needs.contains(FormatNeeds::PANES);
        if windows && !self.format_universe_covers(FormatNeeds::WINDOWS)
            || panes && !self.format_universe_covers(FormatNeeds::PANES)
        {
            return false;
        }
        if self.format_universe.engine.is_none()
            && (windows
                && parse_session(&self.session_id).is_none_or(|session| {
                    !self
                        .format_universe
                        .parts
                        .windows
                        .lock()
                        .contains_key(&session)
                })
                || panes
                    && parse_window(&self.window_id).is_none_or(|window| {
                        !self
                            .format_universe
                            .parts
                            .panes
                            .lock()
                            .contains_key(&window)
                    }))
        {
            return false;
        }
        (!windows || self.format_universe.windows(&self.session_id).is_some())
            && (!panes || self.format_universe.panes(&self.window_id).is_some())
    }
}

impl<'e> StatusContext<'e> {
    pub(super) fn borrowed(
        engine: &'e MuxEngine,
        session: Option<SessionId>,
        window: Option<WindowId>,
        pane: Option<PaneId>,
        format_client: FormatClient,
        format_universe: FormatUniverseRef<'e>,
    ) -> Self {
        Self {
            values: OnceLock::new(),
            tree: Some(FormatTree {
                engine,
                session,
                window,
                pane,
                format_client,
            }),
            variables: Arc::default(),
            format_client,
            capture_revision: None,
            session_id: session.map(|id| id.to_string()).unwrap_or_default(),
            window_id: window.map(|id| id.to_string()).unwrap_or_default(),
            pane_id: pane.map(|id| id.to_string()).unwrap_or_default(),
            format_now: i64::try_from(engine.format_now())
                .ok()
                .filter(|time| *time != 0),
            format_universe,
        }
    }

    pub(super) fn borrowed_child<'a>(&self, engine: Option<&'a MuxEngine>) -> StatusContext<'a> {
        if let Some(engine) = engine.filter(|_| tree::borrowed_formats()) {
            let mut child = StatusContext::borrowed(
                engine,
                parse_session(&self.session_id),
                parse_window(&self.window_id),
                parse_pane(&self.pane_id),
                self.format_client,
                FormatUniverseRef::default(),
            );
            child.variables.clone_from(&self.variables);
            child
        } else if tree::borrowed_formats() {
            self.detach_values(None)
        } else {
            let mut context = StatusContext::from((**self).clone());
            context.variables.clone_from(&self.variables);
            context.format_client = self.format_client;
            context.session_id.clone_from(&self.session_id);
            context.window_id.clone_from(&self.window_id);
            context.pane_id.clone_from(&self.pane_id);
            context.format_now = self.format_now;
            context.format_universe.engine = engine;
            context
        }
    }
}

const PANE_HOOK_FORMAT_VARIABLES: [&str; 17] = [
    "pane_kind",
    "agent_state",
    "agent_pending_permission",
    "agent_parent",
    "browser_url",
    "pane_pb_state",
    "pane_pb_progress",
    "pane_last_command_status",
    "pane_status",
    "pane_status_kind",
    "pane_status_progress",
    "pane_status_app",
    "pane_status_title",
    "pane_status_message",
    "pane_status_raw_title",
    "pane_status_raw_message",
    "pane_status_reported",
];

impl StatusContext<'_> {
    pub fn scoped_format_values(
        &self,
        scope: &str,
        hooks: &mut impl StatusHooks,
    ) -> serde_json::Value {
        let scope = match scope {
            "session" => FormatScope::Session,
            "window" => FormatScope::Window,
            "pane" => FormatScope::Pane,
            "client" => FormatScope::Client,
            _ => return serde_json::Value::Object(serde_json::Map::new()),
        };
        let context = ResolvedFormatContext {
            values: self.clone(),
            has_session: !self.session_id.is_empty()
                || !self.session_name.is_empty()
                || self.active_window_index.is_some(),
            has_window: !self.window_id.is_empty()
                || !self.window_name.is_empty()
                || self.window_width.is_some(),
            has_pane: !self.pane_id.is_empty()
                || !self.pane_title.is_empty()
                || self.pane_width.is_some(),
            format_type: match scope {
                FormatScope::Session => FormatType::Session,
                FormatScope::Window => FormatType::Window,
                FormatScope::Pane => FormatType::Pane,
                _ => FormatType::None,
            },
        };
        let mut expander = Expander {
            context: &context,
            universe: &context.values.format_universe,
            hooks,
            time: false,
            job_tag: FormatJobTag::None,
            client_row: None,
            trace: None,
        };
        serde_json::Value::Object(
            FORMAT_VARIABLES
                .iter()
                .filter(|variable| {
                    variable.scope == scope
                        || (scope == FormatScope::Pane && variable.scope == FormatScope::Terminal)
                })
                .map(|variable| variable.name)
                .chain(
                    PANE_HOOK_FORMAT_VARIABLES
                        .into_iter()
                        .filter(|_| scope == FormatScope::Pane),
                )
                .map(|name| {
                    let value = expander
                        .lookup(name, &ModifierFlags::default())
                        .unwrap_or_default();
                    (
                        name.to_owned(),
                        serde_json::Value::String(value.to_string()),
                    )
                })
                .collect(),
        )
    }
}

pub(super) fn optional_display<T: ToString>(value: Option<T>) -> Cow<'static, str> {
    value.map_or(Cow::Borrowed(""), |value| Cow::Owned(value.to_string()))
}

pub(super) fn optional_bool(value: Option<bool>) -> Cow<'static, str> {
    value.map_or(Cow::Borrowed(""), |value| Cow::Borrowed(bool_string(value)))
}

pub(super) const fn bool_string(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}
