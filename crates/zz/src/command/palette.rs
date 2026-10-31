use std::{rc::Rc, sync::Arc};

use ala::{App, Entity, KeyDownEvent, SharedString, Subscription};
use penna::IconName;
use zz_client::{
    completion::PaneKindAvailability, gui_actions::GuiOffer, place::ActionKind, recents::Recents,
};
use zz_protocol::{
    CommandInvocation, InputMessage, MuxSnapshot, PaneId, PaneStatusState, PathEntry, PathListRoot,
};
use zz_ui::{
    command::{
        GuiLevel, PaletteActionLevel, PaletteBackend, PaletteHostId, PaletteSettings,
        PaletteStatus, PaletteTarget, PaletteTree, PaletteTreeHost, PaletteTreePane,
        PaletteTreeSession, PaletteTreeWindow, palette_chats,
    },
    files::{FilesAction, ListingSink, ListingUpdate, listing_pane},
    target::TargetPicker,
};

pub(crate) use zz_ui::command::{CommandPaletteEvent, CommandPaletteView, GuiEvent, PaletteMode};

use crate::{
    mux::{
        client::{HostPathListUpdate, MuxClient, PathPickerUpdate},
        hosts::{HostId, HostState},
        nav::{MuxTreeHost, MuxTreeModel, MuxTreePane, MuxTreePaneKind, TreeNode, TreeTarget},
        prefix::terminal_key_input,
    },
    terminal::view::TERMINAL_FONT,
};

pub(crate) fn init_recents(cx: &mut App) {
    let path = match zz_protocol::app_identity::palette_recents_path() {
        Ok(path) => path,
        Err(error) => {
            log::warn!(target: "zz::palette", "palette recents are not persisted: {error}");
            return;
        }
    };
    let recents = std::fs::read_to_string(&path)
        .map_or_else(|_| Recents::default(), |text| Recents::from_json(&text));
    zz_ui::command::install_recents(
        recents,
        move |recents, cx| {
            let path = path.clone();
            let json = recents.to_json();
            cx.background_executor()
                .spawn(async move {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let temporary = path.with_extension("json.tmp");
                    if let Err(error) = std::fs::write(&temporary, json)
                        .and_then(|()| std::fs::rename(&temporary, &path))
                    {
                        log::warn!(target: "zz::palette", "saving palette recents failed: {error}");
                    }
                })
                .detach();
        },
        cx,
    );
}

pub(crate) fn palette_backend(mux: Entity<MuxClient>) -> Rc<dyn PaletteBackend> {
    Rc::new(MuxPalette::new(mux))
}

pub(crate) fn palette_backend_with(
    mux: Entity<MuxClient>,
    gui: Vec<GuiOffer>,
) -> Rc<dyn PaletteBackend> {
    Rc::new(MuxPalette { mux, gui })
}

struct MuxPalette {
    mux: Entity<MuxClient>,
    gui: Vec<GuiOffer>,
}

impl MuxPalette {
    const fn new(mux: Entity<MuxClient>) -> Self {
        Self {
            mux,
            gui: Vec::new(),
        }
    }
}

fn host_name(host: HostId, name: &str) -> String {
    if host == HostId::LOCAL {
        crate::workspace::sidebar::local_hostname().to_string()
    } else {
        name.to_owned()
    }
}

pub(crate) fn path_insert_text(root: &PathListRoot, entry: &PathEntry) -> Option<String> {
    zz_client::path_insert::insert_text(
        &root.root,
        &entry.rel,
        entry.kind,
        root.cwd.as_deref(),
        root.insert,
        false,
    )
}

impl PaletteBackend for MuxPalette {
    fn verbs(&self, cx: &App) -> Vec<zz_client::verbs::Verb> {
        let gui = self
            .gui
            .iter()
            .map(|offer| offer.action.id)
            .collect::<Vec<_>>();
        zz_client::verbs::verbs(
            self.availability(cx),
            &gui,
            true,
            &self.mux.read(cx).command_aliases(),
        )
    }

    fn action_level(&self, kind: ActionKind, _: &mut App) -> Option<Box<dyn PaletteActionLevel>> {
        match kind {
            ActionKind::Gui => Some(Box::new(GuiLevel::new(self.gui.clone()))),
            ActionKind::Files => Some(Box::new(FilesAction::new(palette_backend(
                self.mux.clone(),
            )))),
            ActionKind::Command => None,
        }
    }

    fn snapshot(&self, cx: &App) -> Arc<MuxSnapshot> {
        self.mux.read(cx).snapshot()
    }

    fn host_snapshot<'a>(&self, host: PaletteHostId, cx: &'a App) -> Option<&'a MuxSnapshot> {
        let host = HostId::from(host);
        self.mux
            .read(cx)
            .fleet_hosts()
            .find(|(id, ..)| *id == host)
            .and_then(|(.., snapshot)| snapshot)
    }

    fn tree(&self, cx: &App) -> PaletteTree {
        let mux = self.mux.read(cx);
        let attached = mux.attached_host();
        let mut chats = mux
            .fleet_hosts()
            .filter(|(_, _, state, _)| matches!(state, HostState::Connected))
            .filter_map(|(host, _, _, snapshot)| Some((host, snapshot?)))
            .flat_map(|(host, snapshot)| palette_chats(host.into(), &snapshot.sessions))
            .collect::<Vec<_>>();
        let here = PaletteHostId::from(attached);
        chats.sort_by_key(|chat| chat.host != here);
        PaletteTree {
            chats,
            chat_mode: crate::config::resolved_config(cx).flat_sidebar.value,
            attached: attached.into(),
            hosts: MuxTreeModel::from_mux(mux)
                .hosts
                .iter()
                .map(|host| PaletteTreeHost {
                    id: host.id.into(),
                    name: host_name(host.id, &host.name),
                    detail: host_detail(host, cx),
                    right: match host.state {
                        HostState::Connected => format!(
                            "{} session{}",
                            host.sessions.len(),
                            if host.sessions.len() == 1 { "" } else { "s" }
                        ),
                        HostState::Connecting => "connecting…".to_owned(),
                        HostState::Reconnecting { .. } => "reconnecting…".to_owned(),
                        HostState::Incompatible { .. } => "incompatible version".to_owned(),
                        HostState::Missing => "zz not installed · ↵ to install".to_owned(),
                        HostState::Installing => "installing zz…".to_owned(),
                        _ => "offline · ↵ to connect".to_owned(),
                    },
                    status: match host.state {
                        HostState::Connected => PaletteStatus::Online,
                        HostState::Connecting
                        | HostState::Reconnecting { .. }
                        | HostState::Installing => PaletteStatus::Waiting,
                        _ => PaletteStatus::Offline,
                    },
                    sessions: host
                        .sessions
                        .iter()
                        .map(|session| PaletteTreeSession {
                            id: session.id,
                            name: session.label.to_string(),
                            tmux_name: session.name.clone(),
                            active: session.active,
                            windows: session
                                .windows
                                .iter()
                                .map(|window| PaletteTreeWindow {
                                    id: window.id,
                                    index: window.index,
                                    name: window.name.clone(),
                                    active: window.active,
                                    active_pane: window.active_pane,
                                    panes: window
                                        .panes
                                        .iter()
                                        .map(|pane| tree_pane(pane, host.id == attached, mux))
                                        .collect(),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn settings(&self, cx: &App) -> PaletteSettings {
        PaletteSettings {
            grouped: crate::config::palette_window_layout(cx)
                == crate::config::PaletteWindowLayout::Grouped,
            show_keys: crate::config::palette_show_keys(cx),
            vim: crate::config::palette_vim_mode(cx),
        }
    }

    fn availability(&self, cx: &App) -> PaneKindAvailability {
        PaneKindAvailability {
            browser: crate::browser::controller::is_available(cx),
            editor: crate::config::editor_pane_enabled(cx),
        }
    }

    fn command_shortcut(&self, command: &str, cx: &App) -> Option<SharedString> {
        let mux = self.mux.read(cx);
        mux.prefix_bindings()
            .iter()
            .find(|binding| binding.commands.len() == 1 && binding.commands[0].name == command)
            .map(|binding| {
                format!(
                    "{} {}",
                    mux.canonical_prefix()
                        .unwrap_or_else(|| "prefix".to_owned()),
                    binding.key
                )
                .into()
            })
    }

    fn mono_font(&self, _: &App) -> SharedString {
        TERMINAL_FONT.into()
    }

    fn send_input(&self, input: InputMessage, cx: &mut App) {
        self.mux.read(cx).send_input(input);
    }

    fn send_key(&self, pane: PaneId, event: &KeyDownEvent, cx: &mut App) {
        self.mux.read(cx).send_input(InputMessage::Key {
            pane,
            input: terminal_key_input(&event.keystroke, zz_terminal::KeyAction::Press),
            text_follows: false,
        });
    }

    fn active_pane(&self, cx: &App) -> Option<PaneId> {
        self.mux.read(cx).active_pane()
    }

    fn execute(&self, host: PaletteHostId, command: CommandInvocation, cx: &mut App) {
        self.mux.read(cx).execute_on_host(host.into(), command);
    }

    fn connect_host(&self, host: PaletteHostId, cx: &mut App) {
        self.mux
            .update(cx, |mux, cx| mux.install_or_retry_host(host.into(), cx));
    }

    fn activate(&self, host: PaletteHostId, target: PaletteTarget, cx: &mut App) -> bool {
        let mux = self.mux.read(cx);
        let target = match target {
            PaletteTarget::Session(id) => TreeTarget::Session(id),
            PaletteTarget::Window(id) => TreeTarget::Window(id),
            PaletteTarget::Pane(id) => TreeTarget::Pane(id),
        };
        let Some(activation) = MuxTreeModel::from_mux(mux).activation_for_node(
            TreeNode::Target(host.into(), target),
            mux.attached_host(),
            mux.attached_session(),
        ) else {
            return false;
        };
        crate::mux::nav::activate_nav(&self.mux, activation, cx);
        true
    }

    fn activate_host(&self, host: PaletteHostId, cx: &mut App) -> bool {
        let mux = self.mux.read(cx);
        let Some(activation) = MuxTreeModel::from_mux(mux).activation_for_node(
            TreeNode::Host(host.into()),
            mux.attached_host(),
            mux.attached_session(),
        ) else {
            return false;
        };
        crate::mux::nav::activate_nav(&self.mux, activation, cx);
        true
    }

    fn list_path(
        &self,
        host: PaletteHostId,
        dir: &str,
        anchor: Option<PaneId>,
        cx: &mut App,
    ) -> Result<u64, SharedString> {
        let host = HostId::from(host);
        let pane = {
            let mux = self.mux.read(cx);
            let Some((_, name, _, snapshot)) = mux.fleet_hosts().find(|(id, ..)| *id == host)
            else {
                return Err("No such host".into());
            };
            let name = host_name(host, name);
            let pane = snapshot
                .and_then(|snapshot| listing_pane(snapshot, anchor))
                .ok_or_else(|| format!("{name} has no terminal to list files through"))?;
            (pane, name)
        };
        let (pane, name) = pane;
        self.mux
            .update(cx, |mux, _| {
                mux.request_host_path_list(host, pane, dir.to_owned())
            })
            .ok_or_else(|| format!("{name} is not connected").into())
    }

    fn cancel_path(&self, host: PaletteHostId, request_id: u64, cx: &mut App) {
        self.mux.update(cx, |mux, _| {
            mux.cancel_host_path_list(host.into(), request_id);
        });
    }

    fn feed_targets(&self, picker: &Entity<TargetPicker>, cx: &mut App) -> Option<Subscription> {
        let picker = picker.downgrade();
        Some(
            cx.subscribe(&self.mux, move |mux, event: &HostPathListUpdate, cx| {
                let Some(host) = mux
                    .read(cx)
                    .fleet_hosts()
                    .find(|(host, ..)| *host == event.host)
                    .map(|(host, name, ..)| host_name(host, name))
                else {
                    return;
                };
                let _ = picker.update(cx, |picker, cx| match &event.update {
                    PathPickerUpdate::Begin { request_id, result } => {
                        picker.apply_begin(&host, *request_id, result.clone(), cx);
                    }
                    PathPickerUpdate::Chunk {
                        request_id,
                        entries,
                        done,
                        truncated,
                    } => picker.apply_chunk(&host, *request_id, entries, *done, *truncated, cx),
                    PathPickerUpdate::Git { request_id, marks } => {
                        picker.apply_git(&host, *request_id, marks.clone(), cx);
                    }
                    PathPickerUpdate::Open { .. } | PathPickerUpdate::Close => {}
                });
            }),
        )
    }

    fn feed_listings(&self, sink: ListingSink, cx: &mut App) -> Option<Subscription> {
        Some(
            cx.subscribe(&self.mux, move |_, event: &HostPathListUpdate, cx| {
                let (request_id, update) = match &event.update {
                    PathPickerUpdate::Begin { request_id, result } => {
                        (*request_id, ListingUpdate::Begin(result.clone()))
                    }
                    PathPickerUpdate::Chunk {
                        request_id,
                        entries,
                        done,
                        truncated,
                    } => (
                        *request_id,
                        ListingUpdate::Chunk {
                            entries: Arc::clone(entries),
                            done: *done,
                            truncated: *truncated,
                        },
                    ),
                    PathPickerUpdate::Git { request_id, marks } => {
                        (*request_id, ListingUpdate::Git(marks.clone()))
                    }
                    PathPickerUpdate::Open { .. } | PathPickerUpdate::Close => return,
                };
                sink(event.host.into(), request_id, update, cx);
            }),
        )
    }

    fn home_dir(&self, host: PaletteHostId, _: &App) -> Option<String> {
        (HostId::from(host) == HostId::LOCAL)
            .then(std::env::home_dir)
            .flatten()
            .and_then(|home| home.to_str().map(str::to_owned))
    }

    fn insert_path(
        &self,
        pane: PaneId,
        host: PaletteHostId,
        root: &PathListRoot,
        entry: &PathEntry,
        cx: &mut App,
    ) {
        let mux = self.mux.read(cx);
        let host = HostId::from(host);
        let text = if host == mux.attached_host() {
            path_insert_text(root, entry)
        } else {
            let name = mux
                .fleet_hosts()
                .find(|(id, ..)| *id == host)
                .map(|(id, name, ..)| host_name(id, name));
            name.map(|name| {
                let path = format!(
                    "{}/{}",
                    root.root.trim_end_matches('/'),
                    entry.rel.trim_end_matches('/')
                );
                format!("{name}:{path} ")
            })
        };
        if let Some(text) = text {
            mux.paste_to_pane(pane, text);
        }
    }
}

fn host_detail(host: &MuxTreeHost, cx: &App) -> String {
    if host.id == HostId::LOCAL {
        if cfg!(target_os = "macos") {
            "This Mac"
        } else {
            "This computer"
        }
        .to_owned()
    } else {
        crate::config::fleet_hosts(cx)
            .iter()
            .find(|entry| entry.name == host.name)
            .map_or_else(
                || "Remote host".to_owned(),
                |entry| entry.endpoint.to_string(),
            )
    }
}

fn tree_pane(pane: &MuxTreePane, attached: bool, mux: &MuxClient) -> PaletteTreePane {
    let attention = attached
        .then(|| mux.agent_attention_status(pane.id))
        .flatten();
    PaletteTreePane {
        id: pane.id,
        index: pane.index,
        label: pane.label.clone(),
        program: pane.program.map(|program| program.name.into()),
        detail: match pane.kind {
            MuxTreePaneKind::Terminal => match attention {
                Some(zz_client::AgentAttentionStatus::Working) => "Agent · running",
                Some(zz_client::AgentAttentionStatus::NeedsInput) => "Agent · waiting for input",
                Some(zz_client::AgentAttentionStatus::Idle) => "Agent · idle",
                Some(zz_client::AgentAttentionStatus::Failed) => "Agent · failed",
                None => "Terminal",
            },
            MuxTreePaneKind::Browser => "Browser",
            MuxTreePaneKind::Editor => "Editor",
            MuxTreePaneKind::Picker => "New pane",
        }
        .into(),
        icon: match pane.kind {
            MuxTreePaneKind::Picker => IconName::Plus,
            MuxTreePaneKind::Terminal => IconName::SquareTerminal,
            MuxTreePaneKind::Browser => IconName::Globe,
            MuxTreePaneKind::Editor => IconName::File,
        },
        running: attention == Some(zz_client::AgentAttentionStatus::Working),
        needs_you: matches!(
            pane.status,
            Some(PaneStatusState::Blocked | PaneStatusState::Error)
        ),
        terminal: pane.kind == MuxTreePaneKind::Terminal,
        cwd: pane.cwd.clone(),
    }
}
