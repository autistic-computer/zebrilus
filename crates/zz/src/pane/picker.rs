use ala::{
    AnyElement, App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, KeyBinding,
    KeyDownEvent, Keystroke, MouseButton, NoAction, Render, Window, prelude::*,
};
use penna::IconName;
use zz_client::new_chat::{
    AGENT_CHOICES, AgentChoice, ChatKey, ChatPicker, ChatStage, FolderChoice, MachineChoice,
    agent_picker_place, close_picker_command, folder_start, landing_chat_commands, picker_folders,
    picker_in_split, recent_folders, start_agent_commands,
};
use zz_protocol::{CommandInvocation, MuxSnapshot, PaneId, SessionId, WindowId};
use zz_ui::pane::{ChatPickerRow, ChatPickerTarget, chat_picker_rows, pane_picker_row};

use crate::mux::client::MuxClient;
use crate::mux::hosts::HostId;
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
}

const CHOICES: [PaneChoice; 3] = [
    PaneChoice::Terminal,
    PaneChoice::Browser,
    PaneChoice::Editor,
];

fn choices(cx: &App) -> Vec<PaneChoice> {
    CHOICES
        .into_iter()
        .filter(|choice| match choice {
            PaneChoice::Terminal => true,
            PaneChoice::Browser => browser::controller::is_available(cx),
            PaneChoice::Editor => config::editor_pane_enabled(cx),
        })
        .collect()
}

impl PaneChoice {
    const fn command_argument(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Browser => "browser",
            Self::Editor => "editor",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Browser => "Browser",
            Self::Editor => "Editor",
        }
    }

    const fn shortcut(self) -> &'static str {
        match self {
            Self::Terminal => "t",
            Self::Browser => "b",
            Self::Editor => "e",
        }
    }

    const fn icon(self) -> IconName {
        match self {
            Self::Terminal => IconName::SquareTerminal,
            Self::Browser => zz_ui::CHROME_ICON,
            Self::Editor => IconName::File,
        }
    }

    const fn element_id(self) -> &'static str {
        match self {
            Self::Terminal => "pane-picker-terminal",
            Self::Browser => "pane-picker-browser",
            Self::Editor => "pane-picker-editor",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerAction {
    Activate(PaneChoice),
    Move(isize),
    Confirm,
    Close,
}

fn picker_action(keystroke: &Keystroke, choices: &[PaneChoice]) -> Option<PickerAction> {
    let modifiers = keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform {
        return None;
    }
    if let Some(choice) = choices
        .iter()
        .find(|choice| choice.shortcut() == keystroke.key)
    {
        return Some(PickerAction::Activate(*choice));
    }
    match keystroke.key.as_str() {
        "j" | "down" => Some(PickerAction::Move(1)),
        "k" | "up" => Some(PickerAction::Move(-1)),
        "enter" => Some(PickerAction::Confirm),
        "escape" => Some(PickerAction::Close),
        _ => None,
    }
}

fn wrapping_step(selected: usize, delta: isize, len: usize) -> usize {
    (selected + len).wrapping_add_signed(delta) % len
}

pub(crate) struct ChooseFolder {
    pub(crate) start: String,
}

pub(crate) struct AgentChosen {
    pub(crate) window: WindowId,
    pub(crate) from: SessionId,
}

pub(crate) struct ChooseMachine {
    pub(crate) host: HostId,
    pub(crate) pane: Option<PaneId>,
}

struct Chat {
    place: Option<(SessionId, WindowId)>,
    folders: Vec<FolderChoice>,
    machines: Vec<MachineChoice>,
}

fn chat_picker(snapshot: &MuxSnapshot, pane: Option<PaneId>, chat_view: bool) -> Option<Chat> {
    let Some(pane) = pane else {
        return Some(Chat {
            place: None,
            folders: recent_folders(snapshot),
            machines: Vec::new(),
        });
    };
    agent_picker_place(snapshot, pane, chat_view).map(|place| Chat {
        place: Some(place),
        folders: picker_folders(snapshot, pane),
        machines: Vec::new(),
    })
}

fn machines(mux: &MuxClient) -> Vec<MachineChoice> {
    let attached = mux.attached_host();
    mux.fleet_hosts()
        .map(|(host, name, _, snapshot)| MachineChoice {
            host: u64::from(zz_ui::command::PaletteHostId::from(host).0),
            name: if host == HostId::LOCAL {
                crate::workspace::sidebar::local_hostname().to_string()
            } else {
                name.to_owned()
            },
            remote: host != HostId::LOCAL,
            live: snapshot.is_some(),
            current: host == attached,
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
    selected: usize,
    window_corners: WindowCorners,
    chat: ChatPicker,
    chat_view: bool,
}

impl EventEmitter<AgentChosen> for PanePickerView {}

impl EventEmitter<ChooseFolder> for PanePickerView {}

impl EventEmitter<ChooseMachine> for PanePickerView {}

impl PanePickerView {
    pub(crate) fn new(pane: PaneId, mux: ala::Entity<MuxClient>, cx: &mut Context<Self>) -> Self {
        Self::build(Some(pane), mux, cx)
    }

    pub(crate) fn empty(mux: ala::Entity<MuxClient>, cx: &mut Context<Self>) -> Self {
        Self::build(None, mux, cx)
    }

    fn build(pane: Option<PaneId>, mux: ala::Entity<MuxClient>, cx: &mut Context<Self>) -> Self {
        Self {
            pane,
            mux,
            focus_handle: cx.focus_handle(),
            selected: 0,
            window_corners: WindowCorners::NONE,
            chat: ChatPicker::default(),
            chat_view: false,
        }
    }

    pub(crate) fn set_chat_view(&mut self, chat_view: bool, cx: &mut Context<Self>) {
        if self.chat_view != chat_view {
            self.chat_view = chat_view;
            self.selected = 0;
            cx.notify();
        }
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

    fn activate(&self, choice: PaneChoice, cx: &Context<Self>) {
        if let Some(pane) = self.pane {
            self.mux.read(cx).execute(materialize_command(pane, choice));
        }
    }

    fn close(&self, cx: &Context<Self>) {
        let mux = self.mux.read(cx);
        if let Some(command) = self
            .pane
            .and_then(|pane| close_picker_command(&mux.snapshot(), pane))
        {
            mux.execute(command);
        }
    }

    fn chat(&self, cx: &App) -> Option<Chat> {
        let mux = self.mux.read(cx);
        chat_picker(&mux.snapshot(), self.pane, self.chat_view).map(|chat| Chat {
            machines: machines(mux),
            ..chat
        })
    }

    fn choose_machine(&mut self, chat: &Chat, index: usize, cx: &mut Context<Self>) {
        let Some(machine) = chat.machines.get(index) else {
            return;
        };
        if !machine.current && machine.live {
            cx.emit(ChooseMachine {
                host: machine_host(machine),
                pane: self.pane,
            });
        }
        self.chat.open(ChatStage::Agents);
        cx.notify();
    }

    fn open_machines(&mut self, chat: &Chat, cx: &mut Context<Self>) {
        if chat.machines.len() > 1 {
            self.chat.open(ChatStage::Machines);
            self.chat.selected = chat
                .machines
                .iter()
                .position(|machine| machine.current)
                .unwrap_or_default();
            cx.notify();
        }
    }

    fn start(&self, chat: &Chat, agent: AgentChoice, cx: &mut Context<Self>) {
        let mux = self.mux.read(cx);
        let folder = self.chat.folder(&chat.folders);
        let launches = crate::agent::launch::load();
        let (Some(pane), Some((from, window))) = (self.pane, chat.place) else {
            for command in landing_chat_commands(&mux.snapshot(), folder, agent, &launches) {
                mux.execute(command);
            }
            return;
        };
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
            ChatKey::Terminal => self.activate(PaneChoice::Terminal, cx),
            ChatKey::Browser => self.activate(PaneChoice::Browser, cx),
            ChatKey::Close => self.close(cx),
            ChatKey::Changed => cx.notify(),
            ChatKey::Ignored => return false,
        }
        true
    }

    pub(crate) fn forget_folder(&mut self, cx: &mut Context<Self>) {
        self.chat.folder = None;
        self.chat.open(ChatStage::Agents);
        cx.notify();
    }

    pub(crate) fn pick_folder(&mut self, folder: &str, cx: &mut Context<Self>) {
        self.chat.pick(folder);
        cx.notify();
    }

    fn choose_folder(&self, chat: &Chat, cx: &mut Context<Self>) {
        cx.emit(ChooseFolder {
            start: folder_start(self.chat.folder(&chat.folders)),
        });
    }

    fn sync_offers(&mut self, cx: &App) {
        let split = self
            .pane
            .is_some_and(|pane| picker_in_split(&self.mux.read(cx).snapshot(), pane));
        self.chat.terminal = split;
        self.chat.browser = split && browser::controller::is_available(cx);
    }

    fn chat_rows(&mut self, chat: &Chat, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.sync_offers(cx);
        chat_picker_rows(
            self.id(),
            &self.chat,
            &chat.folders,
            &chat.machines,
            true,
            cx,
        )
        .into_iter()
        .map(|row| match row {
            ChatPickerRow::Label(label) => label.into_any_element(),
            ChatPickerRow::Item(target, row) => {
                let view = cx.entity();
                let hover_view = view.clone();
                row.on_mouse_move(move |_, _, cx| {
                    hover_view.update(cx, |picker, cx| {
                        let index = match target {
                            ChatPickerTarget::Agent(index, _)
                            | ChatPickerTarget::Machine(index) => index,
                            ChatPickerTarget::Terminal => AGENT_CHOICES.len(),
                            ChatPickerTarget::Browser => {
                                AGENT_CHOICES.len() + usize::from(picker.chat.terminal)
                            }
                            ChatPickerTarget::MachineLine | ChatPickerTarget::FolderLine => return,
                        };
                        if picker.chat.selected != index {
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
                            ChatPickerTarget::MachineLine => picker.open_machines(&chat, cx),
                            ChatPickerTarget::Machine(index) => {
                                picker.choose_machine(&chat, index, cx);
                            }
                            ChatPickerTarget::FolderLine => picker.choose_folder(&chat, cx),
                            ChatPickerTarget::Agent(_, agent) => picker.start(&chat, agent, cx),
                            ChatPickerTarget::Terminal => {
                                picker.activate(PaneChoice::Terminal, cx);
                            }
                            ChatPickerTarget::Browser => picker.activate(PaneChoice::Browser, cx),
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
        if let Some(chat) = self.chat(cx) {
            let keystroke = &event.keystroke;
            let modifiers = keystroke.modifiers;
            let modified = modifiers.control || modifiers.alt || modifiers.platform;
            self.chat.machines = chat.machines.len();
            self.sync_offers(cx);
            let opening = self.chat.stage != ChatStage::Machines;
            let outcome =
                self.chat
                    .key(&keystroke.key, keystroke.key_char.as_deref(), modified, &[]);
            if opening && self.chat.stage == ChatStage::Machines {
                self.open_machines(&chat, cx);
            }
            if self.settle(outcome, &chat, cx) {
                cx.stop_propagation();
            }
            return;
        }
        let choices = choices(cx);
        match picker_action(&event.keystroke, &choices) {
            Some(PickerAction::Activate(choice)) => self.activate(choice, cx),
            Some(PickerAction::Move(delta)) => {
                self.selected = wrapping_step(self.selected, delta, choices.len());
                cx.notify();
            }
            Some(PickerAction::Confirm) => {
                if let Some(choice) = choices.get(self.selected).copied() {
                    self.activate(choice, cx);
                }
            }
            Some(PickerAction::Close) => self.close(cx),
            None => return,
        }
        cx.stop_propagation();
    }

    fn row(&self, index: usize, choice: PaneChoice, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected == index;
        let view = cx.entity();
        let hover_view = view.clone();
        pane_picker_row(
            (choice.element_id(), self.id()),
            choice.title(),
            choice.icon(),
            choice.shortcut(),
            selected,
            true,
            cx,
        )
        .on_mouse_move(move |_, _, cx| {
            hover_view.update(cx, |picker, cx| {
                if picker.selected != index {
                    picker.selected = index;
                    cx.notify();
                }
            });
        })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            view.update(cx, |picker, cx| picker.activate(choice, cx));
            cx.stop_propagation();
        })
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
        let rows = if let Some(chat) = self.chat(cx) {
            self.chat_rows(&chat, cx)
        } else {
            let choices = choices(cx);
            self.selected = self.selected.min(choices.len() - 1);
            choices
                .iter()
                .enumerate()
                .map(|(index, choice)| self.row(index, *choice, cx).into_any_element())
                .collect::<Vec<_>>()
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
    use ala::Modifiers;

    use super::*;

    fn key(value: &str) -> Keystroke {
        Keystroke {
            modifiers: Modifiers::default(),
            key: value.to_owned(),
            key_char: Some(value.to_owned()),
        }
    }

    #[test]
    fn direct_shortcuts_activate_each_choice_and_escape_closes() {
        assert_eq!(
            CHOICES,
            [
                PaneChoice::Terminal,
                PaneChoice::Browser,
                PaneChoice::Editor,
            ],
            "the ungated picker catalog contains all three pane kinds"
        );
        assert_eq!(
            picker_action(&key("t"), &CHOICES),
            Some(PickerAction::Activate(PaneChoice::Terminal))
        );
        assert_eq!(
            picker_action(&key("b"), &CHOICES),
            Some(PickerAction::Activate(PaneChoice::Browser))
        );
        assert_eq!(
            picker_action(&key("e"), &CHOICES),
            Some(PickerAction::Activate(PaneChoice::Editor))
        );
        assert_eq!(
            picker_action(&key("escape"), &CHOICES),
            Some(PickerAction::Close)
        );
    }

    #[test]
    fn pane_choices_use_pane_specific_icons() {
        assert_eq!(PaneChoice::Browser.icon(), zz_ui::CHROME_ICON);
        assert_eq!(PaneChoice::Editor.icon(), IconName::File);
    }

    #[test]
    fn gated_out_choices_take_their_hotkeys_with_them() {
        let editor_gated = [PaneChoice::Terminal, PaneChoice::Browser];
        assert_eq!(picker_action(&key("e"), &editor_gated), None);
        assert_eq!(
            picker_action(&key("t"), &editor_gated),
            Some(PickerAction::Activate(PaneChoice::Terminal))
        );
    }

    #[test]
    fn vertical_keys_move_the_cursor_and_enter_takes_it() {
        for key_name in ["j", "down"] {
            assert_eq!(
                picker_action(&key(key_name), &CHOICES),
                Some(PickerAction::Move(1))
            );
        }
        for key_name in ["k", "up"] {
            assert_eq!(
                picker_action(&key(key_name), &CHOICES),
                Some(PickerAction::Move(-1))
            );
        }
        assert_eq!(
            picker_action(&key("enter"), &CHOICES),
            Some(PickerAction::Confirm)
        );
    }

    #[test]
    fn horizontal_keys_do_not_drive_the_picker() {
        for key_name in ["left", "right", "h", "l"] {
            assert_eq!(picker_action(&key(key_name), &CHOICES), None);
        }
    }

    #[test]
    fn the_cursor_wraps_at_both_ends() {
        assert_eq!(wrapping_step(0, 1, CHOICES.len()), 1);
        assert_eq!(wrapping_step(CHOICES.len() - 1, 1, CHOICES.len()), 0);
        assert_eq!(wrapping_step(0, -1, CHOICES.len()), CHOICES.len() - 1);
        assert_eq!(wrapping_step(1, -1, CHOICES.len()), 0);
    }

    #[test]
    fn command_modifiers_do_not_drive_the_picker() {
        let mut keystroke = key("t");
        keystroke.modifiers.platform = true;
        assert_eq!(picker_action(&keystroke, &CHOICES), None);
    }

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
