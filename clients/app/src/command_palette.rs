use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use ala::{App, Entity, KeyDownEvent, SharedString, Subscription};
use penna::blocks::palette::PaletteStatus;
use penna::{ActiveTheme as _, IconName};
use zz_client::completion::PaneKindAvailability;
use zz_client::navigation::{ordered_panes, pane_index, pane_label, session_label};
use zz_client::{
    AgentAttentionStatus, ClientCore, CoreEvent, agent_attention_status, gui_actions::GuiOffer,
    place::ActionKind, sidebar::SidebarTarget,
};
use zz_protocol::{
    CommandInvocation, InputMessage, MuxOptionKey, MuxSnapshot, PaneId, PaneKindSnapshot,
    PathEntry, PathListRoot, ProtocolMessage, SessionId, WindowId,
};
use zz_terminal::TerminalViewAction;
use zz_ui::{
    command::{
        GuiLevel, PaletteActionLevel, PaletteBackend, PaletteHostId, PaletteSettings,
        PaletteTarget, PaletteTree, PaletteTreeHost, PaletteTreePane, PaletteTreeSession,
        PaletteTreeWindow, palette_chats,
    },
    files::{FilesAction, ListingSink, ListingUpdate, listing_pane},
};

pub(crate) use zz_ui::command::{CommandPaletteEvent, CommandPaletteView, PaletteMode};

use crate::{
    app::HostPage,
    connection::Connection,
    roster::{HostRequest, Roster},
};

#[cfg(any(target_os = "ios", target_os = "android"))]
fn host_name(page: &HostPage) -> String {
    page.title.clone()
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn host_name(_: &HostPage) -> String {
    "local".to_owned()
}

static PATH_LISTS: AtomicU64 = AtomicU64::new(0);

#[must_use]
pub(crate) fn palette_backend(
    connection: Entity<Connection>,
    roster: Entity<Roster>,
    settings: PaletteSettings,
) -> Rc<dyn PaletteBackend> {
    palette_backend_with(connection, roster, settings, Vec::new(), false)
}

#[must_use]
pub(crate) fn palette_backend_with(
    connection: Entity<Connection>,
    roster: Entity<Roster>,
    settings: PaletteSettings,
    gui: Vec<GuiOffer>,
    chat_mode: bool,
) -> Rc<dyn PaletteBackend> {
    Rc::new(ConnectionPalette {
        connection,
        roster,
        settings,
        gui,
        chat_mode,
    })
}

struct ConnectionPalette {
    connection: Entity<Connection>,
    roster: Entity<Roster>,
    settings: PaletteSettings,
    gui: Vec<GuiOffer>,
    chat_mode: bool,
}

impl PaletteBackend for ConnectionPalette {
    fn verbs(&self, cx: &App) -> Vec<zz_client::verbs::Verb> {
        let gui = self
            .gui
            .iter()
            .map(|offer| offer.action.id)
            .collect::<Vec<_>>();
        let aliases = self
            .connection
            .read(cx)
            .core
            .mux_options()
            .get(MuxOptionKey::CommandAlias)
            .map(|option| option.value.clone())
            .unwrap_or_default();
        zz_client::verbs::verbs(self.availability(cx), &gui, true, &aliases)
    }

    fn action_level(&self, kind: ActionKind, _: &mut App) -> Option<Box<dyn PaletteActionLevel>> {
        match kind {
            ActionKind::Gui => Some(Box::new(GuiLevel::new(self.gui.clone()))),
            ActionKind::Files => Some(Box::new(FilesAction::new(palette_backend(
                self.connection.clone(),
                self.roster.clone(),
                self.settings,
            )))),
            ActionKind::Command => None,
        }
    }

    fn snapshot(&self, cx: &App) -> Arc<MuxSnapshot> {
        Arc::clone(self.connection.read(cx).core.snapshot())
    }

    fn host_snapshot<'a>(&self, host: PaletteHostId, cx: &'a App) -> Option<&'a MuxSnapshot> {
        if host == self.here(cx) {
            return Some(self.connection.read(cx).core.snapshot());
        }
        Some(
            self.roster
                .read(cx)
                .connection(host)?
                .read(cx)
                .core
                .snapshot(),
        )
    }

    fn tree(&self, cx: &App) -> PaletteTree {
        let here = self.here(cx);
        let hosts = self.hosts(cx);
        let mut chats = hosts
            .iter()
            .filter_map(|page| Some((page.id, page.connection.as_ref()?.read(cx))))
            .filter(|(_, connection)| connection.connected)
            .flat_map(|(host, connection)| {
                palette_chats(host, &connection.core.snapshot().sessions)
            })
            .collect::<Vec<_>>();
        chats.sort_by_key(|chat| chat.host != here);
        PaletteTree {
            chats,
            chat_mode: self.chat_mode,
            attached: here,
            hosts: hosts
                .iter()
                .map(|page| tree_host(page, page.id == here, cx))
                .collect(),
        }
    }

    fn settings(&self, _: &App) -> PaletteSettings {
        self.settings
    }

    fn availability(&self, _: &App) -> PaneKindAvailability {
        PaneKindAvailability {
            browser: cfg!(any(target_os = "ios", target_os = "android")),
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

    fn execute(&self, host: PaletteHostId, command: CommandInvocation, cx: &mut App) {
        let Some(connection) = self.host_connection(host, cx) else {
            return;
        };
        connection.update(cx, |connection, cx| connection.invoke(command, cx));
    }

    fn connect_host(&self, host: PaletteHostId, cx: &mut App) {
        Roster::request(&self.roster, HostRequest::Connect(host), cx);
    }

    fn activate(&self, host: PaletteHostId, target: PaletteTarget, cx: &mut App) -> bool {
        if host != self.here(cx) {
            let found = self
                .host_connection(host, cx)
                .map(|connection| connection.read(cx))
                .filter(|connection| connection.connected)
                .is_some_and(|connection| place(connection.core.snapshot(), target).is_some());
            if found {
                Roster::request(
                    &self.roster,
                    HostRequest::Open(host, sidebar_target(target)),
                    cx,
                );
            }
            return found;
        }
        let connection = self.connection.read(cx);
        if !connection.connected {
            return false;
        }
        let Some((session, window, pane)) = place(connection.core.snapshot(), target) else {
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

    fn activate_host(&self, host: PaletteHostId, cx: &mut App) -> bool {
        let live = host != self.here(cx)
            && self
                .host_connection(host, cx)
                .is_some_and(|connection| connection.read(cx).connected);
        if live {
            Roster::request(
                &self.roster,
                HostRequest::Open(host, SidebarTarget::Host),
                cx,
            );
        }
        live
    }

    fn list_path(
        &self,
        host: PaletteHostId,
        dir: &str,
        anchor: Option<PaneId>,
        cx: &mut App,
    ) -> Result<u64, SharedString> {
        let connection = self
            .host_connection(host, cx)
            .filter(|connection| connection.read(cx).connected)
            .ok_or_else(|| SharedString::from("Not connected"))?;
        let pane = listing_pane(connection.read(cx).core.snapshot(), anchor);
        let request_id = PATH_LISTS.fetch_add(1, Ordering::Relaxed) + 1;
        connection.update(cx, |connection, cx| {
            connection.send(
                ProtocolMessage::PathListRequest {
                    request_id,
                    pane,
                    dir: Some(dir.to_owned()),
                },
                cx,
            );
        });
        Ok(request_id)
    }

    fn cancel_path(&self, host: PaletteHostId, request_id: u64, cx: &mut App) {
        if let Some(connection) = self.host_connection(host, cx) {
            connection.update(cx, |connection, cx| {
                connection.send(ProtocolMessage::PathListCancel { request_id }, cx);
            });
        }
    }

    fn feed_listings(&self, sink: ListingSink, cx: &mut App) -> Option<Subscription> {
        let sink = Rc::new(sink);
        let feeds = Rc::new(RefCell::new(
            Vec::<(Entity<Connection>, Subscription)>::new(),
        ));
        let follow = move |roster: Entity<Roster>, cx: &mut App| {
            let connections = roster
                .read(cx)
                .hosts()
                .iter()
                .filter_map(|page| page.connection.clone())
                .collect::<Vec<_>>();
            let mut feeds = feeds.borrow_mut();
            feeds.retain(|(connection, _)| connections.contains(connection));
            for connection in connections {
                if feeds.iter().any(|(fed, _)| *fed == connection) {
                    continue;
                }
                let (sink, roster) = (Rc::clone(&sink), roster.downgrade());
                let feed = cx.subscribe(&connection, move |connection, event: &CoreEvent, cx| {
                    if let CoreEvent::Message(message) = event
                        && let Some((request_id, update)) = listing_update(message)
                        && let Some(host) = roster
                            .upgrade()
                            .and_then(|roster| roster.read(cx).id_of(&connection))
                    {
                        sink(host, request_id, update, cx);
                    }
                });
                feeds.push((connection, feed));
            }
        };
        follow(self.roster.clone(), cx);
        Some(cx.observe(&self.roster, follow))
    }

    fn insert_path(
        &self,
        pane: PaneId,
        _: PaletteHostId,
        root: &PathListRoot,
        entry: &PathEntry,
        cx: &mut App,
    ) {
        if let Some(text) = zz_client::path_insert::insert_text(
            &root.root,
            &entry.rel,
            entry.kind,
            root.cwd.as_deref(),
            root.insert,
            false,
        ) {
            self.send_input(
                InputMessage::TerminalView {
                    pane,
                    action: TerminalViewAction::Paste(text),
                },
                cx,
            );
        }
    }
}

impl ConnectionPalette {
    fn here(&self, cx: &App) -> PaletteHostId {
        self.roster
            .read(cx)
            .id_of(&self.connection)
            .unwrap_or_default()
    }

    fn hosts(&self, cx: &App) -> Vec<HostPage> {
        let roster = self.roster.read(cx);
        if roster.id_of(&self.connection).is_some() {
            return roster.hosts().to_vec();
        }
        vec![HostPage {
            id: PaletteHostId::default(),
            endpoint: None,
            title: String::new(),
            connection: Some(self.connection.clone()),
        }]
    }

    fn host_connection(&self, host: PaletteHostId, cx: &App) -> Option<Entity<Connection>> {
        if host == self.here(cx) {
            return Some(self.connection.clone());
        }
        self.roster.read(cx).connection(host).cloned()
    }
}

const fn sidebar_target(target: PaletteTarget) -> SidebarTarget {
    match target {
        PaletteTarget::Session(id) => SidebarTarget::Session(id),
        PaletteTarget::Window(id) => SidebarTarget::Window(id),
        PaletteTarget::Pane(id) => SidebarTarget::Pane(id),
    }
}

fn place(
    snapshot: &MuxSnapshot,
    target: PaletteTarget,
) -> Option<(SessionId, Option<WindowId>, Option<PaneId>)> {
    snapshot.sessions.iter().find_map(|session| match target {
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
}

fn tree_host(page: &HostPage, here: bool, cx: &App) -> PaletteTreeHost {
    let connection = page
        .connection
        .as_ref()
        .map(|connection| connection.read(cx))
        .filter(|connection| connection.connected);
    let sessions: Vec<_> = connection.map_or_else(Vec::new, |connection| {
        let core = &connection.core;
        let snapshot = core.snapshot();
        snapshot
            .sessions
            .iter()
            .map(|session| {
                let active = here && core.attached_session() == Some(session.id);
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
    });
    let connecting = connection.is_none()
        && page
            .connection
            .as_ref()
            .is_some_and(|connection| connection.read(cx).status == "Connecting…");
    PaletteTreeHost {
        id: page.id,
        name: host_name(page),
        detail: "Daemon host".to_owned(),
        right: if connection.is_some() {
            format!(
                "{} session{}",
                sessions.len(),
                if sessions.len() == 1 { "" } else { "s" }
            )
        } else if connecting {
            "connecting…".to_owned()
        } else {
            "offline · ↵ to connect".to_owned()
        },
        status: if connection.is_some() {
            PaletteStatus::Online
        } else if connecting {
            PaletteStatus::Waiting
        } else {
            PaletteStatus::Offline
        },
        sessions,
    }
}

fn listing_update(message: &ProtocolMessage) -> Option<(u64, ListingUpdate)> {
    Some(match message {
        ProtocolMessage::PathListBegin { request_id, result } => {
            (*request_id, ListingUpdate::Begin(result.clone()))
        }
        ProtocolMessage::PathListChunk {
            request_id,
            entries,
            done,
            truncated,
        } => (
            *request_id,
            ListingUpdate::Chunk {
                entries: entries.clone().into(),
                done: *done,
                truncated: *truncated,
            },
        ),
        ProtocolMessage::PathListGit { request_id, marks } => {
            (*request_id, ListingUpdate::Git(marks.clone()))
        }
        _ => return None,
    })
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
            PaneKindSnapshot::Agent(_) => "Agent",
            PaneKindSnapshot::View(view) => view.content.kind_name(),
            PaneKindSnapshot::Other { .. } => "Needs a newer zz",
        }
        .into(),
        icon: match &pane.kind {
            PaneKindSnapshot::Picker => IconName::Plus,
            PaneKindSnapshot::Terminal => IconName::SquareTerminal,
            PaneKindSnapshot::Browser(_) => IconName::Globe,
            PaneKindSnapshot::Editor(_) => IconName::File,
            PaneKindSnapshot::Agent(agent) => zz_ui::pane::agent_provider_icon(agent.provider),
            PaneKindSnapshot::View(_) => IconName::Eye,
            PaneKindSnapshot::Other { .. } => IconName::Info,
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

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use std::cell::RefCell;

    use ala::{AppContext as _, TestAppContext};
    use zz_protocol::{
        Event, EventPayload, InsertStyle, LayoutNode, PaneSnapshot, PaneSummary, PathKind,
        SessionId, SessionSnapshot, ShellKind, WindowId, WindowSnapshot,
    };

    use super::*;

    const HOST: PaletteHostId = PaletteHostId(0);

    fn snapshot(pane: PaneId) -> MuxSnapshot {
        MuxSnapshot {
            generation: 1,
            sessions: vec![SessionSnapshot {
                id: SessionId(1),
                name: "work".into(),
                active_window: WindowId(1),
                viewers: Vec::new(),
                windows: vec![WindowSnapshot {
                    id: WindowId(1),
                    index: 0,
                    name: "zsh".into(),
                    automatic_rename: true,
                    active_pane: pane,
                    zoomed_pane: None,
                    layout: LayoutNode::Pane(pane),
                    panes: [(
                        pane,
                        PaneSnapshot {
                            id: pane,
                            title: "zsh".into(),
                            kind: PaneKindSnapshot::Terminal,
                            synchronized_input: false,
                            bell: false,
                            dead: false,
                            dead_status: None,
                            border_colour: None,
                            active_border_colour: None,
                            border_status_text: String::new(),
                            mode: None,
                            status: None,
                            summary: PaneSummary {
                                cwd: "/home/me/src".to_owned(),
                                ..PaneSummary::default()
                            },
                        },
                    )]
                    .into(),
                    layout_dump: String::new(),
                    visible_layout_dump: String::new(),
                    status_label: String::new(),
                    activity: false,
                    silence: false,
                    pane_border_status: zz_protocol::PaneBorderStatus::default(),
                    pane_border_lines: zz_protocol::PaneBorderLines::default(),
                    pane_border_indicators: zz_protocol::PaneBorderIndicators::default(),
                    pane_order: vec![pane],
                    pane_z_order: vec![pane],
                    floating: Vec::new(),
                    modal: None,
                    sx: 0,
                    sy: 0,
                    pane_base_index: 0,
                }],
            }],
            focused_window: Some(WindowId(1)),
        }
    }

    type Sent = Rc<RefCell<Vec<ProtocolMessage>>>;

    fn live(pane: PaneId, cx: &mut TestAppContext) -> (Entity<Connection>, Sent) {
        let connection = cx.new(Connection::new);
        let sent = connection.update(cx, |connection, cx| {
            let sent = connection.record_input_for_test();
            connection.connected = true;
            connection.handle_message_for_test(
                ProtocolMessage::Event(Event {
                    sequence: 1,
                    payload: EventPayload::Snapshot(snapshot(pane)),
                }),
                cx,
            );
            sent
        });
        (connection, sent)
    }

    fn page(id: u32, connection: &Entity<Connection>) -> HostPage {
        HostPage {
            id: PaletteHostId(id),
            endpoint: Some(format!("host-{id}")),
            title: format!("host-{id}"),
            connection: Some(connection.clone()),
        }
    }

    #[ala::test]
    fn listings_and_commands_go_to_the_host_they_name(cx: &mut TestAppContext) {
        let (here, here_sent) = live(PaneId(1), cx);
        let (other, other_sent) = live(PaneId(1), cx);
        let roster = cx.new(|_| Roster::default());
        let pages = vec![page(7, &here), page(3, &other)];
        roster.update(cx, |roster, cx| roster.sync(pages, cx));
        let backend = palette_backend(here, roster, PaletteSettings::default());
        let fed = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&fed);
        let _feed = cx.update(|cx| {
            backend.feed_listings(
                Box::new(move |host, request, _, _| seen.borrow_mut().push((host, request))),
                cx,
            )
        });
        let before = here_sent.borrow().len();
        let request = cx
            .update(|cx| backend.list_path(PaletteHostId(3), "~/", None, cx))
            .expect("listed");
        let kill = CommandInvocation::new("kill-window", ["-t", "@1"]);
        cx.update(|cx| backend.execute(PaletteHostId(3), kill.clone(), cx));
        assert_eq!(here_sent.borrow().len(), before);
        assert!(matches!(
            other_sent.borrow().as_slice(),
            [
                ProtocolMessage::PathListRequest { request_id, pane: Some(PaneId(1)), .. },
                ProtocolMessage::CommandRequest(sent),
            ] if *request_id == request && sent.command == kill
        ));
        other.update(cx, |connection, cx| {
            connection.handle_message_for_test(
                ProtocolMessage::PathListGit {
                    request_id: request,
                    marks: Vec::new(),
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(fed.borrow().as_slice(), [(PaletteHostId(3), request)]);
    }

    #[ala::test]
    fn files_are_listed_through_a_terminal_and_pasted_into_the_pane(cx: &mut TestAppContext) {
        let pane = PaneId(1);
        let (connection, sent) = live(pane, cx);
        let roster = cx.new(|_| Roster::default());
        let pages = vec![page(0, &connection)];
        roster.update(cx, |roster, cx| roster.sync(pages, cx));
        let backend = palette_backend(connection.clone(), roster, PaletteSettings::default());
        let fed = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&fed);
        let _feed = cx.update(|cx| {
            backend.feed_listings(
                Box::new(move |host, request, update, _| {
                    seen.borrow_mut().push((host, request, update));
                }),
                cx,
            )
        });
        let request = cx
            .update(|cx| backend.list_path(HOST, "~/src/", Some(PaneId(9)), cx))
            .expect("listed");
        assert_eq!(
            sent.borrow().last(),
            Some(&ProtocolMessage::PathListRequest {
                request_id: request,
                pane: Some(pane),
                dir: Some("~/src/".to_owned()),
            }),
            "a pane that is not a terminal here lists through one that is"
        );

        let root = PathListRoot {
            root: "/home/me/src".to_owned(),
            display_root: "~/src".to_owned(),
            cwd: Some("/home/me/src".to_owned()),
            insert: InsertStyle::Shell(ShellKind::Posix),
        };
        connection.update(cx, |connection, cx| {
            connection.handle_message_for_test(
                ProtocolMessage::PathListBegin {
                    request_id: request,
                    result: Ok(root.clone()),
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert!(matches!(
            fed.borrow().as_slice(),
            [(HOST, id, ListingUpdate::Begin(Ok(_)))] if *id == request
        ));

        let entry = PathEntry {
            rel: "my notes.txt".to_owned(),
            kind: PathKind::File,
            symlink: false,
        };
        cx.update(|cx| backend.insert_path(pane, HOST, &root, &entry, cx));
        assert_eq!(
            sent.borrow().last(),
            Some(&ProtocolMessage::Input(InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::Paste("'my notes.txt' ".to_owned()),
            }))
        );

        let mut pickers_only = snapshot(pane);
        if let Some(only) = pickers_only.sessions[0].windows[0].panes.get_mut(&pane) {
            only.kind = PaneKindSnapshot::Picker;
        }
        connection.update(cx, |connection, cx| {
            connection.handle_message_for_test(
                ProtocolMessage::Event(Event {
                    sequence: 2,
                    payload: EventPayload::Snapshot(pickers_only),
                }),
                cx,
            );
        });
        let request = cx
            .update(|cx| backend.list_path(HOST, "~/", Some(pane), cx))
            .expect("a named folder lists with no terminal open");
        assert_eq!(
            sent.borrow().last(),
            Some(&ProtocolMessage::PathListRequest {
                request_id: request,
                pane: None,
                dir: Some("~/".to_owned()),
            })
        );
    }
}
