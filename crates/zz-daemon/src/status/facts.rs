use super::*;

#[derive(Clone, Default)]
pub(crate) struct FormatHookFacts {
    pub(crate) terminals: Arc<BTreeMap<PaneId, Arc<TerminalSession>>>,
    pub(crate) pane_pipes: Arc<BTreeMap<PaneId, u32>>,
    pub(crate) session_attachments: Arc<BTreeMap<SessionId, (usize, String)>>,
    pub(crate) session_last_attached: Arc<BTreeMap<SessionId, u64>>,
    pub(crate) unseen_changes: Arc<BTreeSet<PaneId>>,
    pub(crate) window_clients: Arc<BTreeMap<WindowId, Vec<String>>>,
    pub(crate) buffer: Option<BufferFormatFacts>,
    pub(crate) client: Option<ClientFormatFacts>,
    pub(crate) clients: Arc<Vec<FormatClientRow>>,
    pub(crate) client_environment: Option<Arc<ClientEnvironmentBlob>>,
    pub(crate) message: Option<MessageFormatFacts>,
    pub(crate) mux: Arc<zz_mux::FormatFacts>,
    pub(crate) copy_modes: Arc<BTreeMap<PaneId, Vec<(String, Arc<CopyModeFacts>)>>>,
    pub(crate) pane_modes: Arc<BTreeMap<PaneId, (usize, &'static str)>>,
}

impl FormatHookFacts {
    pub(crate) fn shared_empty() -> Self {
        static EMPTY: LazyLock<FormatHookFacts> = LazyLock::new(FormatHookFacts::default);
        EMPTY.clone()
    }
}

pub(crate) trait FormatFactSource {
    fn terminals(&self) -> &BTreeMap<PaneId, Arc<TerminalSession>>;
    fn pane_pipes(&self) -> &BTreeMap<PaneId, u32>;
    fn session_attachments(&self) -> &BTreeMap<SessionId, (usize, String)>;
    fn session_last_attached(&self) -> &BTreeMap<SessionId, u64>;
    fn unseen_changes(&self) -> &BTreeSet<PaneId>;
    fn window_clients(&self, _context: &StatusContext) -> &BTreeMap<WindowId, Vec<String>>;
    fn buffer(&self) -> Option<&BufferFormatFacts>;
    fn client(&self) -> Option<&ClientFormatFacts>;
    fn clients(&self, _context: &StatusContext) -> &[FormatClientRow];
    fn client_environment(&self) -> Option<&Arc<ClientEnvironmentBlob>>;
    fn message(&self) -> Option<&MessageFormatFacts>;
    fn mux(&self) -> &zz_mux::FormatFacts;
    fn copy_modes(&self) -> &BTreeMap<PaneId, Vec<(String, Arc<CopyModeFacts>)>>;
    fn pane_modes(&self) -> &BTreeMap<PaneId, (usize, &'static str)>;

    fn pane_in_mode_count(&self, pane: PaneId) -> usize {
        self.pane_modes().get(&pane).map_or(0, |(count, _)| *count)
            + usize::from(self.copy_modes().contains_key(&pane))
    }
}

impl FormatFactSource for FormatHookFacts {
    fn terminals(&self) -> &BTreeMap<PaneId, Arc<TerminalSession>> {
        &self.terminals
    }

    fn pane_pipes(&self) -> &BTreeMap<PaneId, u32> {
        &self.pane_pipes
    }

    fn session_attachments(&self) -> &BTreeMap<SessionId, (usize, String)> {
        &self.session_attachments
    }

    fn session_last_attached(&self) -> &BTreeMap<SessionId, u64> {
        &self.session_last_attached
    }

    fn unseen_changes(&self) -> &BTreeSet<PaneId> {
        &self.unseen_changes
    }

    fn window_clients(&self, _context: &StatusContext) -> &BTreeMap<WindowId, Vec<String>> {
        &self.window_clients
    }

    fn buffer(&self) -> Option<&BufferFormatFacts> {
        self.buffer.as_ref()
    }

    fn client(&self) -> Option<&ClientFormatFacts> {
        self.client.as_ref()
    }

    fn clients(&self, _context: &StatusContext) -> &[FormatClientRow] {
        &self.clients
    }

    fn client_environment(&self) -> Option<&Arc<ClientEnvironmentBlob>> {
        self.client_environment.as_ref()
    }

    fn message(&self) -> Option<&MessageFormatFacts> {
        self.message.as_ref()
    }

    fn mux(&self) -> &zz_mux::FormatFacts {
        &self.mux
    }

    fn copy_modes(&self) -> &BTreeMap<PaneId, Vec<(String, Arc<CopyModeFacts>)>> {
        &self.copy_modes
    }

    fn pane_modes(&self) -> &BTreeMap<PaneId, (usize, &'static str)> {
        &self.pane_modes
    }
}

#[derive(Clone)]
pub(crate) struct BufferFormatFacts {
    pub(crate) name: String,
    pub(crate) data: Arc<[u8]>,
    pub(crate) created: SystemTime,
}

#[derive(Clone, Default)]
pub(crate) struct ClientFormatFacts {
    pub(crate) activity: String,
    pub(crate) cell_height: String,
    pub(crate) cell_width: String,
    pub(crate) colours: String,
    pub(crate) control_mode: String,
    pub(crate) created: String,
    pub(crate) discarded: String,
    pub(crate) flags: String,
    pub(crate) height: String,
    pub(crate) key_table: String,
    pub(crate) last_session: String,
    pub(crate) name: String,
    pub(crate) pid: String,
    pub(crate) prefix: String,
    pub(crate) readonly: String,
    pub(crate) session: String,
    pub(crate) termfeatures: String,
    pub(crate) termname: String,
    pub(crate) termtype: String,
    pub(crate) theme: String,
    pub(crate) tty: String,
    pub(crate) uid: String,
    pub(crate) user: String,
    pub(crate) utf8: String,
    pub(crate) width: String,
    pub(crate) written: String,
    pub(crate) line: usize,
    pub(crate) environment: Option<Arc<ClientEnvironmentBlob>>,
    pub(crate) terminal: Option<Arc<TtyTerm>>,
    pub(crate) viewport: Option<ClientViewportFacts>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ClientViewportFacts {
    pub(crate) columns: u16,
    pub(crate) rows: u16,
    pub(crate) window_width: u16,
    pub(crate) window_height: u16,
    pub(crate) cursor: Option<(u16, u16)>,
}

impl ClientViewportFacts {
    pub(crate) fn bigger(self) -> bool {
        self.columns < self.window_width || self.rows < self.window_height
    }

    pub(crate) fn offsets(self) -> Option<(u16, u16)> {
        if !self.bigger() {
            return None;
        }
        let Some((cursor_x, cursor_y)) = self.cursor else {
            return Some((0, 0));
        };
        let offset_x = if cursor_x < self.columns {
            0
        } else if cursor_x > self.window_width.saturating_sub(self.columns) {
            self.window_width.saturating_sub(self.columns)
        } else {
            cursor_x.saturating_sub(self.columns / 2)
        };
        let offset_y = if cursor_y < self.rows {
            0
        } else if cursor_y > self.window_height.saturating_sub(self.rows) {
            self.window_height.saturating_sub(self.rows)
        } else {
            cursor_y.saturating_sub(self.rows).saturating_add(1)
        };
        Some((offset_x, offset_y))
    }
}

const DEFAULT_XPIXEL: u32 = 16;

const DEFAULT_YPIXEL: u32 = 32;

pub(crate) fn client_environment_rows(
    environment: Option<&Arc<ClientEnvironmentBlob>>,
) -> Vec<FormatEnvironRow> {
    environment.map_or_else(Vec::new, |environment| {
        environment
            .map()
            .iter()
            .map(|(name, value)| FormatEnvironRow {
                name: name.to_string(),
                value: value.clone(),
                hidden: false,
                removed: false,
            })
            .collect()
    })
}

impl ClientFormatFacts {
    pub(crate) fn loop_row(&self, activity: u64) -> FormatClientRow {
        FormatClientRow {
            name: self.name.clone(),
            activity,
            environment: client_environment_rows(self.environment.as_ref()),
            variables: BTreeMap::from([
                ("client_activity".to_owned(), self.activity.clone()),
                ("client_cell_height".to_owned(), self.cell_height.clone()),
                ("client_cell_width".to_owned(), self.cell_width.clone()),
                ("client_colours".to_owned(), self.colours.clone()),
                ("client_control_mode".to_owned(), self.control_mode.clone()),
                ("client_created".to_owned(), self.created.clone()),
                ("client_discarded".to_owned(), self.discarded.clone()),
                ("client_flags".to_owned(), self.flags.clone()),
                ("client_height".to_owned(), self.height.clone()),
                ("client_key_table".to_owned(), self.key_table.clone()),
                ("client_last_session".to_owned(), self.last_session.clone()),
                ("client_name".to_owned(), self.name.clone()),
                ("client_pid".to_owned(), self.pid.clone()),
                ("client_prefix".to_owned(), self.prefix.clone()),
                ("client_readonly".to_owned(), self.readonly.clone()),
                ("client_session".to_owned(), self.session.clone()),
                ("client_termfeatures".to_owned(), self.termfeatures.clone()),
                ("client_termname".to_owned(), self.termname.clone()),
                ("client_termtype".to_owned(), self.termtype.clone()),
                ("client_theme".to_owned(), self.theme.clone()),
                ("client_tty".to_owned(), self.tty.clone()),
                ("client_uid".to_owned(), self.uid.clone()),
                ("client_user".to_owned(), self.user.clone()),
                ("client_utf8".to_owned(), self.utf8.clone()),
                ("client_width".to_owned(), self.width.clone()),
                ("client_written".to_owned(), self.written.clone()),
            ]),
        }
    }
}

#[derive(Clone)]
pub(crate) struct MessageFormatFacts {
    pub(crate) number: u64,
    pub(crate) text: String,
    pub(crate) time: SystemTime,
}

pub(super) fn window_scoped(context: &StatusContext) -> bool {
    !context.window_id.is_empty()
        || context
            .variable("window_name")
            .is_some_and(|value| !value.is_empty())
        || context
            .variable("window_width")
            .is_some_and(|value| !value.is_empty())
}

pub(super) fn window_cell_pixels(client: Option<&ClientFormatFacts>, width: bool) -> String {
    let default = if width {
        DEFAULT_XPIXEL
    } else {
        DEFAULT_YPIXEL
    };
    client
        .map(|client| {
            if width {
                &client.cell_width
            } else {
                &client.cell_height
            }
        })
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|pixels| *pixels != 0)
        .unwrap_or(default)
        .to_string()
}
