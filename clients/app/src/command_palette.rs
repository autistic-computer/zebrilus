use std::{rc::Rc, sync::Arc};

use zz_client::completion::PaneKindAvailability;
use zz_client::navigation::{ordered_panes, pane_index, pane_label, session_label};
use zz_client::{AgentAttentionStatus, ClientCore, agent_attention_status};
use zz_gpui::{App, Entity, KeyDownEvent, SharedString};
use zz_protocol::{
    CommandInvocation, InputMessage, MuxOptionKey, MuxSnapshot, PaneId, PaneKindSnapshot,
    ProtocolMessage,
};
use zz_ui::{
    ActiveTheme as _, IconName,
    command::{
        PaletteBackend, PaletteHostId, PaletteSettings, PaletteStatus, PaletteTarget, PaletteTree,
        PaletteTreeHost, PaletteTreePane, PaletteTreeSession, PaletteTreeWindow,
    },
};

pub(crate) use zz_ui::command::{CommandPaletteEvent, CommandPaletteView, PaletteMode};

use crate::connection::Connection;

const HOST: PaletteHostId = PaletteHostId(0);

#[cfg(target_os = "ios")]
fn host_name(connection: &Connection) -> String {
    connection
        .endpoint()
        .map_or_else(|| "host".to_owned(), crate::app::host_title)
}

#[cfg(not(target_os = "ios"))]
fn host_name(_: &Connection) -> String {
    "local".to_owned()
}

#[must_use]
pub fn palette_backend(
    connection: Entity<Connection>,
    settings: PaletteSettings,
) -> Rc<dyn PaletteBackend> {
    Rc::new(ConnectionPalette {
        connection,
        settings,
    })
}

struct ConnectionPalette {
    connection: Entity<Connection>,
    settings: PaletteSettings,
}

impl PaletteBackend for ConnectionPalette {
    fn snapshot(&self, cx: &App) -> Arc<MuxSnapshot> {
        Arc::clone(self.connection.read(cx).core.snapshot())
    }

    fn host_snapshot<'a>(&self, _: PaletteHostId, cx: &'a App) -> Option<&'a MuxSnapshot> {
        Some(self.connection.read(cx).core.snapshot())
    }

    fn tree(&self, cx: &App) -> PaletteTree {
        let connection = self.connection.read(cx);
        let core = &connection.core;
        let snapshot = core.snapshot();
        let sessions: Vec<_> = if connection.connected {
            snapshot
                .sessions
                .iter()
                .map(|session| {
                    let active = core.attached_session() == Some(session.id);
                    let focused = snapshot.focused_window_for(session);
                    PaletteTreeSession {
                        id: session.id,
                        name: session_label(session).to_string(),
                        tmux_name: session.name.clone(),
                        active,
                        windows: session
                            .windows
                            .iter()
                            .map(|window| PaletteTreeWindow {
                                id: window.id,
                                index: window.index,
                                name: window.name.clone(),
                                active: active && window.id == focused,
                                active_pane: window.active_pane,
                                panes: ordered_panes(window)
                                    .into_iter()
                                    .map(|pane| tree_pane(window, pane, core))
                                    .collect(),
                            })
                            .collect(),
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        PaletteTree {
            attached: HOST,
            hosts: vec![PaletteTreeHost {
                id: HOST,
                name: host_name(connection),
                detail: "Daemon host".to_owned(),
                right: if connection.connected {
                    format!(
                        "{} session{}",
                        sessions.len(),
                        if sessions.len() == 1 { "" } else { "s" }
                    )
                } else {
                    "offline · ↵ to connect".to_owned()
                },
                status: if connection.connected {
                    PaletteStatus::Online
                } else {
                    PaletteStatus::Offline
                },
                sessions,
            }],
        }
    }

    fn settings(&self, _: &App) -> PaletteSettings {
        self.settings
    }

    fn availability(&self, _: &App) -> PaneKindAvailability {
        PaneKindAvailability {
            browser: cfg!(target_os = "ios"),
            editor: false,
        }
    }

    fn command_shortcut(&self, command: &str, cx: &App) -> Option<SharedString> {
        let core = &self.connection.read(cx).core;
        core.prefix_bindings()
            .iter()
            .find(|binding| binding.commands.len() == 1 && binding.commands[0].name == command)
            .map(|binding| {
                format!(
                    "{} {}",
                    core.mux_options().get(MuxOptionKey::Prefix).map_or_else(
                        || "prefix".to_owned(),
                        |option| zz_protocol::canonical_key(&option.value)
                    ),
                    binding.key
                )
                .into()
            })
    }

    fn mono_font(&self, cx: &App) -> SharedString {
        cx.theme().mono_font_family.clone()
    }

    fn send_input(&self, input: InputMessage, cx: &mut App) {
        self.connection.update(cx, |connection, cx| {
            connection.send(ProtocolMessage::Input(input), cx);
        });
    }

    fn send_key(&self, pane: PaneId, event: &KeyDownEvent, cx: &mut App) {
        self.send_input(
            InputMessage::Key {
                pane,
                input: crate::terminal::key_input(event),
                text_follows: false,
            },
            cx,
        );
    }

    fn active_pane(&self, cx: &App) -> Option<PaneId> {
        let core = &self.connection.read(cx).core;
        let snapshot = core.snapshot();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == core.attached_session())?;
        let focused = snapshot.focused_window_for(session);
        session
            .windows
            .iter()
            .find(|window| window.id == focused)
            .map(|window| window.active_pane)
    }

    fn execute(&self, _: PaletteHostId, command: CommandInvocation, cx: &mut App) {
        self.connection.update(cx, |connection, cx| {
            connection.command(
                &command.name,
                command
                    .args
                    .iter()
                    .map(|arg| arg.as_str().to_owned())
                    .collect(),
                cx,
            );
        });
    }

    fn connect_host(&self, _: PaletteHostId, cx: &mut App) {
        self.connection.update(cx, Connection::reconnect);
    }

    fn activate(&self, _: PaletteHostId, target: PaletteTarget, cx: &mut App) -> bool {
        let connection = self.connection.read(cx);
        if !connection.connected {
            return false;
        }
        let Some((session, window, pane)) =
            connection
                .core
                .snapshot()
                .sessions
                .iter()
                .find_map(|session| match target {
                    PaletteTarget::Session(id) => (id == session.id).then_some((id, None, None)),
                    PaletteTarget::Window(id) => session
                        .windows
                        .iter()
                        .find(|window| window.id == id)
                        .map(|window| (session.id, Some(window.id), None)),
                    PaletteTarget::Pane(id) => session
                        .windows
                        .iter()
                        .find(|window| window.panes.contains_key(&id))
                        .map(|window| (session.id, Some(window.id), Some(id))),
                })
        else {
            return false;
        };
        self.connection.update(cx, |connection, cx| {
            if connection.core.attached_session() != Some(session) {
                connection.attach(session, cx);
            }
            if let Some(window) = window {
                connection.command("select-window", vec!["-t".into(), window.to_string()], cx);
            }
            if let Some(pane) = pane {
                connection.command(
                    "select-pane",
                    vec!["-Z".into(), "-t".into(), pane.to_string()],
                    cx,
                );
            }
        });
        true
    }
}

fn tree_pane(
    window: &zz_protocol::WindowSnapshot,
    pane: &zz_protocol::PaneSnapshot,
    core: &ClientCore,
) -> PaletteTreePane {
    let attention = matches!(pane.kind, PaneKindSnapshot::Terminal)
        .then(|| core.agent_state(pane.id).map(agent_attention_status))
        .flatten();
    PaletteTreePane {
        id: pane.id,
        index: pane_index(window, pane.id),
        label: pane_label(pane),
        program: zz_client::window_summary::pane_program(pane).map(|program| program.name.into()),
        detail: match &pane.kind {
            PaneKindSnapshot::Terminal => match attention {
                Some(AgentAttentionStatus::Working) => "Agent · running",
                Some(AgentAttentionStatus::NeedsInput) => "Agent · waiting for input",
                Some(AgentAttentionStatus::Idle) => "Agent · idle",
                Some(AgentAttentionStatus::Failed) => "Agent · failed",
                None => "Terminal",
            },
            PaneKindSnapshot::Browser(_) => "Browser · unavailable",
            PaneKindSnapshot::Editor(_) => "Editor · unavailable",
            PaneKindSnapshot::Picker => "New pane",
        }
        .into(),
        icon: match &pane.kind {
            PaneKindSnapshot::Picker => IconName::Plus,
            PaneKindSnapshot::Terminal => IconName::SquareTerminal,
            PaneKindSnapshot::Browser(_) => IconName::Globe,
            PaneKindSnapshot::Editor(_) => IconName::File,
        },
        running: attention == Some(AgentAttentionStatus::Working),
        needs_you: pane.status.as_ref().is_some_and(|status| {
            matches!(
                status.state,
                zz_protocol::PaneStatusState::Blocked | zz_protocol::PaneStatusState::Error
            )
        }),
        terminal: matches!(pane.kind, PaneKindSnapshot::Terminal),
        cwd: pane.summary.cwd.clone(),
    }
}
