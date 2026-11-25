use std::sync::Arc;

use ala::{
    AnyElement, App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, KeyBinding,
    KeyDownEvent, MouseButton, NoAction, Render, Window, prelude::*,
};
use zz_client::new_chat::{
    AGENT_CHOICES, AgentChoice, ChatAgentStatus, ChatKey, ChatPicker, ChatStage, FolderChoice,
    Landing, MachineChoice, MachineChoiceState, agent_picker_place, any_agent_installed,
    chat_agent_status, close_picker_command, landing_chat, landing_pane, picker_folders,
    recent_folders, start_agent_commands,
};
use zz_protocol::{CommandInvocation, MuxSnapshot, PaneId, SessionId, WindowId};
use zz_ui::pane::{ChatPickerRow, ChatPickerTarget, chat_picker_rows};

use crate::mux::client::MuxClient;
use crate::mux::hosts::{HostId, HostState};
use crate::window::corners::WindowCorners;
use crate::{browser, config};

const PICKER_KEY_CONTEXT: &str = "PanePicker";

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", NoAction, Some(PICKER_KEY_CONTEXT)),
        KeyBinding::new("shift-tab", NoAction, Some(PICKER_KEY_CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneChoice {
    Terminal,
    Browser,
    Editor,
    Simulator,
}

impl PaneChoice {
    const fn command_argument(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Browser => "browser",
            Self::Editor => "editor",
            Self::Simulator => zz_protocol::SIMULATOR_PANE_KIND,
        }
    }

    const fn key(self) -> ChatKey {
        match self {
            Self::Terminal => ChatKey::Terminal,
            Self::Browser => ChatKey::Browser,
            Self::Editor => ChatKey::Editor,
            Self::Simulator => ChatKey::Simulator,
        }
    }
}

pub(crate) struct ChooseFolder {
    pub(crate) host: HostId,
    pub(crate) start: String,
}

pub(crate) struct AgentChosen {
    pub(crate) window: WindowId,
    pub(crate) from: SessionId,
}

pub(crate) struct ChooseMachine {
    pub(crate) host: HostId,
}

pub(crate) struct ChatPicked {
    pub(crate) host: HostId,
    pub(crate) landing: Landing,
}

pub(crate) struct LeaveLanding;

struct Chat {
    host: HostId,
    place: Option<(SessionId, WindowId)>,
    folders: Vec<FolderChoice>,
    machines: Vec<MachineChoice>,
}

fn host_snapshot(mux: &MuxClient, host: HostId) -> Option<Arc<MuxSnapshot>> {
    if host == mux.attached_host() {
        Some(mux.snapshot())
    } else {
        mux.host_snapshot(host).cloned()
    }
}

fn machines(mux: &MuxClient, target: HostId) -> Vec<MachineChoice> {
    mux.fleet_hosts()
        .map(|(host, name, host_state, snapshot)| {
            let state = if snapshot.is_some() {
                MachineChoiceState::Live
            } else if matches!(host_state, HostState::Installing) {
                MachineChoiceState::Installing
            } else if matches!(host_state, HostState::Missing) && mux.host_supports_install(host) {
                MachineChoiceState::InstallZz
            } else {
                MachineChoiceState::Offline
            };
            MachineChoice {
                host: u64::from(zz_ui::command::PaletteHostId::from(host).0),
                name: if host == HostId::LOCAL {
                    crate::workspace::sidebar::local_hostname().to_string()
                } else {
                    name.to_owned()
                },
                remote: host != HostId::LOCAL,
                platform: mux.host_platform(host),
                state,
                current: host == target,
            }
        })
        .collect()
}

fn machine_host(machine: &MachineChoice) -> HostId {
    HostId::from(zz_ui::command::PaletteHostId(
        u32::try_from(machine.host).expect("machine choices carry host ids"),
    ))
}

fn materialize_command(pane: PaneId, choice: PaneChoice) -> CommandInvocation {
    CommandInvocation::new(
        "select-pane-kind",
        [
            "-t".to_owned(),
            pane.to_string(),
            choice.command_argument().to_owned(),
        ],
    )
}

fn select_command(pane: PaneId) -> CommandInvocation {
    CommandInvocation::new("select-pane", ["-t".to_owned(), pane.to_string()])
}

pub(crate) struct PanePickerView {
    pane: Option<PaneId>,
    mux: ala::Entity<MuxClient>,
    focus_handle: FocusHandle,
    window_corners: WindowCorners,
    chat: ChatPicker,
    target: Option<HostId>,
    pending_install: Option<(HostId, AgentChoice)>,
    pending_host: Option<HostId>,
}

impl EventEmitter<AgentChosen> for PanePickerView {}

impl EventEmitter<ChooseFolder> for PanePickerView {}

impl EventEmitter<ChooseMachine> for PanePickerView {}

impl EventEmitter<ChatPicked> for PanePickerView {}

impl EventEmitter<LeaveLanding> for PanePickerView {}

impl PanePickerView {
    pub(crate) fn new(pane: PaneId, mux: ala::Entity<MuxClient>, cx: &mut Context<Self>) -> Self {
        Self::build(Some(pane), mux, cx)
    }

    pub(crate) fn empty(mux: ala::Entity<MuxClient>, cx: &mut Context<Self>) -> Self {
        Self::build(None, mux, cx)
    }

    fn build(pane: Option<PaneId>, mux: ala::Entity<MuxClient>, cx: &mut Context<Self>) -> Self {
        cx.observe(&mux, |picker, _, cx| {
            picker.observe_mux(cx);
            cx.notify();
        })
        .detach();
        Self {
            pane,
            mux,
            focus_handle: cx.focus_handle(),
            window_corners: WindowCorners::NONE,
            chat: ChatPicker::default(),
            target: None,
            pending_install: None,
            pending_host: None,
        }
    }

    fn observe_mux(&mut self, cx: &mut Context<Self>) {
        if let Some((host, agent)) = self.pending_install {
            let status = {
                let mux = self.mux.read(cx);
                chat_agent_status(mux.host_providers(host), agent.provider())
            };
            match status {
                ChatAgentStatus::Ready => {
                    self.pending_install = None;
                    if let Some(chat) = self.chat(cx).filter(|chat| chat.host == host) {
                        self.start_agent(&chat, agent, cx);
                    }
                }
                ChatAgentStatus::Failed { .. } => {
                    self.pending_install = None;
                }
                ChatAgentStatus::Installable | ChatAgentStatus::Installing { .. } => {}
            }
        }
        if let Some(host) = self.pending_host {
            let state = self
                .mux
                .read(cx)
                .fleet_hosts()
                .find_map(|(id, _, state, snapshot)| {
                    (id == host).then(|| (state.clone(), snapshot.is_some()))
                });
            match state {
                Some((_, true)) => {
                    self.pending_host = None;
                    cx.emit(ChooseMachine { host });
                }
                Some((
                    HostState::Installing | HostState::Connecting | HostState::Disconnected,
                    _,
                )) => {}
                _ => {
                    self.pending_host = None;
                }
            }
        }
    }

    fn host(&self, mux: &MuxClient) -> HostId {
        self.target
            .filter(|host| self.pane.is_none() && mux.host_snapshot(*host).is_some())
            .unwrap_or_else(|| mux.attached_host())
    }

    pub(crate) fn set_target(&mut self, target: Option<HostId>, cx: &mut Context<Self>) {
        if self.target == target {
            return;
        }
        let before = self.host(self.mux.read(cx));
        self.target = target;
        if self.host(self.mux.read(cx)) != before {
            self.chat.folder = None;
            self.chat.open(ChatStage::Agents);
        }
        cx.notify();
    }

    fn id(&self) -> u64 {
        self.pane.map_or(u64::MAX, |pane| pane.0)
    }

    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus_handle
    }

    pub(crate) fn set_window_corners(&mut self, corners: WindowCorners, cx: &mut Context<Self>) {
        if self.window_corners != corners {
            self.window_corners = corners;
            let view = cx.weak_entity();
            cx.defer(move |cx| {
                view.update(cx, |_, cx| cx.notify()).ok();
            });
        }
    }

    fn activate(&self, chat: &Chat, choice: PaneChoice, cx: &mut Context<Self>) {
        if let Some(pane) = self.pane {
            self.mux.read(cx).execute(materialize_command(pane, choice));
            return;
        }
        let Some(snapshot) = host_snapshot(self.mux.read(cx), chat.host) else {
            return;
        };
        let folder = self.chat.folder(&chat.folders);
        cx.emit(ChatPicked {
            host: chat.host,
            landing: landing_pane(&snapshot, folder, choice.command_argument()),
        });
    }

    fn close(&self, cx: &mut Context<Self>) {
        let Some(pane) = self.pane else {
            cx.emit(LeaveLanding);
            return;
        };
        let mux = self.mux.read(cx);
        if let Some(command) = close_picker_command(&mux.snapshot(), pane) {
            mux.execute(command);
        }
    }

    fn chat(&self, cx: &App) -> Option<Chat> {
        let mux = self.mux.read(cx);
        let host = self.host(mux);
        let snapshot = host_snapshot(mux, host)?;
        let Some(pane) = self.pane else {
            return Some(Chat {
                host,
                place: None,
                folders: recent_folders(&snapshot),
                machines: machines(mux, host),
            });
        };
        agent_picker_place(&snapshot, pane, true).map(|place| Chat {
            host,
            place: Some(place),
            folders: picker_folders(&snapshot, pane),
            machines: Vec::new(),
        })
    }

    fn choose_machine(&mut self, chat: &Chat, index: usize, cx: &mut Context<Self>) {
        let Some(machine) = chat.machines.get(index) else {
            return;
        };
        if machine.current {
            return;
        }
        let host = machine_host(machine);
        match machine.state {
            MachineChoiceState::Live => cx.emit(ChooseMachine { host }),
            MachineChoiceState::InstallZz => {
                self.pending_host = Some(host);
                self.mux
                    .update(cx, |mux, cx| mux.install_zz_on_host(host, cx));
            }
            MachineChoiceState::Installing | MachineChoiceState::Offline => {}
        }
        cx.notify();
    }

    fn start(&mut self, chat: &Chat, agent: AgentChoice, cx: &mut Context<Self>) {
        let status = chat_agent_status(
            self.mux.read(cx).host_providers(chat.host),
            agent.provider(),
        );
        match status {
            ChatAgentStatus::Ready => self.start_agent(chat, agent, cx),
            ChatAgentStatus::Installable | ChatAgentStatus::Failed { .. } => {
                self.pending_install = Some((chat.host, agent));
                self.mux
                    .read(cx)
                    .update_provider(chat.host, agent.provider());
                cx.notify();
            }
            ChatAgentStatus::Installing { .. } => {}
        }
    }

    fn start_agent(&self, chat: &Chat, agent: AgentChoice, cx: &mut Context<Self>) {
        let folder = self.chat.folder(&chat.folders);
        let launches = crate::agent::launch::load();
        let (Some(pane), Some((from, window))) = (self.pane, chat.place) else {
            let Some(snapshot) = host_snapshot(self.mux.read(cx), chat.host) else {
                return;
            };
            cx.emit(ChatPicked {
                host: chat.host,
                landing: landing_chat(&snapshot, folder, agent, &launches),
            });
            return;
        };
        let mux = self.mux.read(cx);
        for command in start_agent_commands(&mux.snapshot(), pane, folder, agent, &launches) {
            mux.execute(command);
        }
        cx.emit(AgentChosen { window, from });
    }

    fn settle(&mut self, outcome: ChatKey, chat: &Chat, cx: &mut Context<Self>) -> bool {
        match outcome {
            ChatKey::Start(agent) => self.start(chat, agent, cx),
            ChatKey::ChooseFolder => self.choose_folder(chat, cx),
            ChatKey::ChooseMachine(index) => self.choose_machine(chat, index, cx),
            ChatKey::NextMachine => {
                if let Some(index) = MachineChoice::next(&chat.machines) {
                    self.choose_machine(chat, index, cx);
                }
            }
            ChatKey::Terminal => self.activate(chat, PaneChoice::Terminal, cx),
            ChatKey::Browser => self.activate(chat, PaneChoice::Browser, cx),
            ChatKey::Editor => self.activate(chat, PaneChoice::Editor, cx),
            ChatKey::Simulator => self.activate(chat, PaneChoice::Simulator, cx),
            ChatKey::Close => self.close(cx),
            ChatKey::Changed => cx.notify(),
            ChatKey::Ignored => return false,
        }
        true
    }

    pub(crate) fn pick_folder(&mut self, folder: &str, cx: &mut Context<Self>) {
        self.chat.pick(folder);
        cx.notify();
    }

    pub(crate) fn follow_folder(&mut self, folder: &str, cx: &mut Context<Self>) {
        self.chat.follow(folder);
        cx.notify();
    }

    fn choose_folder(&self, chat: &Chat, cx: &mut Context<Self>) {
        cx.emit(ChooseFolder {
            host: chat.host,
            start: self.chat.folder(&chat.folders).to_owned(),
        });
    }

    fn sync_offers(&mut self, host: HostId, cx: &App) {
        let pane = self.pane.is_some();
        let offered = pane || !any_agent_installed(self.mux.read(cx).host_providers(host));
        self.chat.terminal = offered;
        self.chat.browser = offered && browser::controller::is_available(cx);
        self.chat.editor = pane && config::editor_pane_enabled(cx);
        self.chat.simulator = pane && cfg!(target_os = "macos");
    }

    fn offer_index(&self, choice: PaneChoice) -> Option<usize> {
        self.chat
            .pane_offers()
            .iter()
            .position(|offer| *offer == choice.key())
            .map(|at| AGENT_CHOICES.len() + at)
    }

    fn chat_rows(&mut self, chat: &Chat, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.mux.read(cx).providers_unrequested() {
            let mux = self.mux.clone();
            cx.defer(move |cx| mux.update(cx, |mux, _| mux.request_providers(false)));
        }
        self.sync_offers(chat.host, cx);
        let providers = self.mux.read(cx).host_providers(chat.host).cloned();
        chat_picker_rows(
            self.id(),
            &self.chat,
            &chat.folders,
            &chat.machines,
            providers.as_ref(),
            true,
            cx,
        )
        .into_iter()
        .map(|row| match row {
            ChatPickerRow::Label(label) => label.into_any_element(),
            ChatPickerRow::Item(target, row) => {
                let choice = match target {
                    ChatPickerTarget::Terminal => Some(PaneChoice::Terminal),
                    ChatPickerTarget::Browser => Some(PaneChoice::Browser),
                    ChatPickerTarget::Editor => Some(PaneChoice::Editor),
                    ChatPickerTarget::Simulator => Some(PaneChoice::Simulator),
                    ChatPickerTarget::Machine(_)
                    | ChatPickerTarget::FolderLine
                    | ChatPickerTarget::Agent(..) => None,
                };
                let view = cx.entity();
                let hover_view = view.clone();
                row.on_mouse_move(move |_, _, cx| {
                    hover_view.update(cx, |picker, cx| {
                        let index = match target {
                            ChatPickerTarget::Agent(index, _) => Some(index),
                            _ => choice.and_then(|choice| picker.offer_index(choice)),
                        };
                        if let Some(index) = index
                            && picker.chat.selected != index
                        {
                            picker.chat.selected = index;
                            cx.notify();
                        }
                    });
                })
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    view.update(cx, |picker, cx| {
                        let Some(chat) = picker.chat(cx) else {
                            return;
                        };
                        match target {
                            ChatPickerTarget::Machine(index) => {
                                picker.choose_machine(&chat, index, cx);
                            }
                            ChatPickerTarget::FolderLine => picker.choose_folder(&chat, cx),
                            ChatPickerTarget::Agent(_, agent) => picker.start(&chat, agent, cx),
                            ChatPickerTarget::Terminal
                            | ChatPickerTarget::Browser
                            | ChatPickerTarget::Editor
                            | ChatPickerTarget::Simulator => {
                                if let Some(choice) = choice {
                                    picker.activate(&chat, choice, cx);
                                }
                            }
                        }
                    });
                    cx.stop_propagation();
                })
                .into_any_element()
            }
        })
        .collect()
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(chat) = self.chat(cx) else {
            return;
        };
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        let modified = modifiers.control || modifiers.alt || modifiers.platform;
        self.chat.machines = chat.machines.len();
        self.sync_offers(chat.host, cx);
        let outcome = self
            .chat
            .key(&keystroke.key, keystroke.key_char.as_deref(), modified, &[]);
        if self.settle(outcome, &chat, cx) {
            cx.stop_propagation();
        }
    }
}

impl Focusable for PanePickerView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PanePickerView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        let select_mux = self.mux.clone();
        let pane = self.pane;
        let rows = match self.chat(cx) {
            Some(chat) => self.chat_rows(&chat, cx),
            None => Vec::new(),
        };
        zz_ui::pane::pane_picker_surface(
            ("pane-picker", self.id()),
            config::pane_content_radii(cx, self.window_corners),
            rows,
            cx,
        )
        .key_context(PICKER_KEY_CONTEXT)
        .track_focus(&self.focus_handle)
        .on_key_down(cx.listener(Self::on_key_down))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if let Some(pane) = pane {
                select_mux.read(cx).execute(select_command(pane));
            }
            focus_handle.focus(window, cx);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_targets_the_existing_picker_pane() {
        assert_eq!(
            materialize_command(PaneId(42), PaneChoice::Browser),
            CommandInvocation::new("select-pane-kind", ["-t", "%42", "browser"])
        );
        assert_eq!(
            materialize_command(PaneId(42), PaneChoice::Editor),
            CommandInvocation::new("select-pane-kind", ["-t", "%42", "editor"])
        );
        assert_eq!(
            select_command(PaneId(7)),
            CommandInvocation::new("select-pane", ["-t", "%7"])
        );
    }
}
