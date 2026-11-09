use crate::connection::Connection;
use ala::{
    AnyElement, App, Context, Corners, ElementId, Entity, FocusHandle, Focusable, KeyDownEvent,
    MouseButton, Pixels, Render, Window, div, prelude::*, px,
};
use penna::{
    Icon, IconName, StyledExt as _,
    blocks::sheet::{bottom_sheet, close_sheet, sheet_close, sheet_row},
};
use zz_client::{
    new_chat::{
        AGENT_CHOICES, AgentChoice, ChatKey, ChatPicker, FolderChoice, agent_picker_place,
        close_picker_command, folder_start, landing_chat_commands, picker_folders, recent_folders,
        start_agent_commands,
    },
    pane_picker::{PaneChoice, PanePickerKey, pane_choices, pane_kind_command, pane_picker_key},
};
use zz_protocol::PaneId;
use zz_ui::pane::{
    ChatPickerRow, ChatPickerTarget, agent_provider_icon, chat_picker_rows, pane_picker_row,
    pane_picker_surface,
};

fn choices() -> &'static [PaneChoice] {
    pane_choices(cfg!(any(target_os = "ios", target_os = "android")))
}

fn choice_icon(choice: &PaneChoice) -> IconName {
    if choice.kind == "browser" {
        IconName::Globe
    } else {
        IconName::SquareTerminal
    }
}

pub(super) struct ChooseFolder {
    pub(super) start: String,
}

pub(super) struct LeaveLanding;

pub(super) struct PanePicker {
    pane: Option<PaneId>,
    connection: Entity<Connection>,
    focus: FocusHandle,
    selected: usize,
    corner_radii: Corners<Pixels>,
    chosen: Option<usize>,
    sheet_closed: bool,
    chat: ChatPicker,
    chat_view: bool,
}

impl PanePicker {
    pub(super) fn new(
        pane: PaneId,
        connection: Entity<Connection>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(Some(pane), connection, cx)
    }

    pub(super) fn landing(connection: Entity<Connection>, cx: &mut Context<Self>) -> Self {
        let mut picker = Self::build(None, connection, cx);
        picker.chat_view = true;
        picker
    }

    fn build(pane: Option<PaneId>, connection: Entity<Connection>, cx: &mut Context<Self>) -> Self {
        Self {
            pane,
            connection,
            focus: cx.focus_handle(),
            selected: 0,
            corner_radii: Corners::default(),
            chosen: None,
            sheet_closed: false,
            chat: ChatPicker::default(),
            chat_view: false,
        }
    }

    pub(super) fn set_chat_view(&mut self, chat_view: bool, cx: &mut Context<Self>) {
        if self.chat_view != chat_view {
            self.chat_view = chat_view;
            self.selected = 0;
            cx.notify();
        }
    }

    fn id(&self) -> u64 {
        self.pane.map_or(u64::MAX, |pane| pane.0)
    }

    fn folders(&self, cx: &App) -> Option<Vec<FolderChoice>> {
        let snapshot = self.connection.read(cx).core.snapshot();
        let Some(pane) = self.pane else {
            return Some(recent_folders(snapshot));
        };
        agent_picker_place(snapshot, pane, self.chat_view).map(|_| picker_folders(snapshot, pane))
    }

    fn close(&self, cx: &mut Context<Self>) {
        let Some(pane) = self.pane else {
            cx.emit(LeaveLanding);
            return;
        };
        let command = close_picker_command(self.connection.read(cx).core.snapshot(), pane);
        if let Some(command) = command {
            self.connection
                .update(cx, |connection, cx| connection.invoke(command, cx));
        }
    }

    fn start(&self, agent: AgentChoice, folders: &[FolderChoice], cx: &mut Context<Self>) {
        let connection = self.connection.read(cx);
        if !connection.connected || connection.core.attached_read_only() {
            return;
        }
        let folder = self.chat.folder(folders);
        let launches = zz_client::agent_launch::AgentLaunches::default();
        let snapshot = connection.core.snapshot();
        let commands = match self.pane {
            Some(pane) => start_agent_commands(snapshot, pane, folder, agent, &launches),
            None => landing_chat_commands(snapshot, folder, agent, &launches),
        };
        self.connection.update(cx, |connection, cx| {
            for command in commands {
                connection.invoke(command, cx);
            }
        });
    }

    fn choose_folder(&self, folders: &[FolderChoice], cx: &mut Context<Self>) {
        cx.emit(ChooseFolder {
            start: folder_start(self.chat.folder(folders)),
        });
    }

    pub(super) fn pick_folder(
        &mut self,
        folder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.chat.pick(folder);
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn chat_rows(
        &self,
        folders: &[FolderChoice],
        writable: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        chat_picker_rows(self.id(), &self.chat, folders, &[], writable, cx)
            .into_iter()
            .map(|row| match row {
                ChatPickerRow::Label(label) => label.into_any_element(),
                ChatPickerRow::Item(target, row) => row
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        let ChatPickerTarget::Agent(index, _) = target else {
                            return;
                        };
                        if this.chat.selected != index {
                            this.chat.selected = index;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            let Some(folders) = this.folders(cx) else {
                                return;
                            };
                            match target {
                                ChatPickerTarget::FolderLine => this.choose_folder(&folders, cx),
                                ChatPickerTarget::Agent(_, agent) => {
                                    this.start(agent, &folders, cx);
                                }
                                ChatPickerTarget::Machine(_)
                                | ChatPickerTarget::Terminal
                                | ChatPickerTarget::Browser => {}
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element(),
            })
            .collect()
    }

    fn sheet_id(&self) -> ElementId {
        ("pane-picker-sheet", self.id()).into()
    }

    pub(super) fn sheet(&self, bottom_inset: Pixels, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.sheet_closed {
            return None;
        }
        let id = self.sheet_id();
        let chat = self.folders(cx).is_some();
        let options = if chat {
            AGENT_CHOICES
                .iter()
                .map(|agent| (agent_provider_icon(agent.provider()), agent.title()))
                .collect::<Vec<_>>()
        } else {
            choices()
                .iter()
                .map(|choice| (choice_icon(choice), choice.title))
                .collect()
        };
        let rows = options
            .into_iter()
            .enumerate()
            .map(|(index, (icon, title))| {
                let id = id.clone();
                sheet_row(
                    ("pane-picker-option", index),
                    Some(Icon::new(icon)),
                    title,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.chosen = Some(index);
                    close_sheet(id.clone(), cx);
                }))
                .into_any_element()
            })
            .collect::<Vec<_>>();
        let close = sheet_close("pane-picker-close").on_click({
            let id = id.clone();
            move |_, _, cx| close_sheet(id.clone(), cx)
        });
        let view = cx.weak_entity();
        Some(
            bottom_sheet(
                id,
                if chat { "New agent" } else { "New pane" },
                [close.into_any_element()],
                div()
                    .flex()
                    .flex_col()
                    .list_inset(cx)
                    .pb(px(8.0))
                    .children(rows),
                bottom_inset,
                move |_, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.sheet_closed = true;
                        match this.chosen.take() {
                            Some(index) => this.activate(index, cx),
                            None => this.close(cx),
                        }
                        cx.notify();
                    });
                },
            )
            .into_any_element(),
        )
    }

    pub(super) fn set_corner_radii(&mut self, radii: Corners<Pixels>, cx: &mut Context<Self>) {
        if self.corner_radii != radii {
            self.corner_radii = radii;
            cx.notify();
        }
    }

    fn activate(&self, index: usize, cx: &mut Context<Self>) {
        if !self.connection.read(cx).connected || self.connection.read(cx).core.attached_read_only()
        {
            return;
        }
        if let Some(folders) = self.folders(cx) {
            if let Some(agent) = AGENT_CHOICES.get(index).copied() {
                self.start(agent, &folders, cx);
            }
            return;
        }
        let (Some(pane), Some(choice)) = (self.pane, choices().get(index)) else {
            return;
        };
        let command = pane_kind_command(pane, choice);
        self.connection
            .update(cx, |connection, cx| connection.invoke(command, cx));
    }

    fn choice_rows(&self, writable: bool, phone: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        choices()
            .iter()
            .enumerate()
            .filter(|_| !phone)
            .map(|(index, choice)| {
                pane_picker_row(
                    ("web-pane-choice", index),
                    choice.title,
                    choice_icon(choice),
                    choice.shortcut,
                    self.selected == index,
                    writable,
                    cx,
                )
                .on_mouse_move(cx.listener(move |this, _, _, cx| {
                    if this.selected != index {
                        this.selected = index;
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.activate(index, cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
            })
            .collect()
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if let Some(folders) = self.folders(cx) {
            let modified = key.modifiers.control || key.modifiers.alt || key.modifiers.platform;
            match self
                .chat
                .key(&key.key, key.key_char.as_deref(), modified, &[])
            {
                ChatKey::Start(agent) => self.start(agent, &folders, cx),
                ChatKey::ChooseFolder => self.choose_folder(&folders, cx),
                ChatKey::Close => self.close(cx),
                ChatKey::Changed => cx.notify(),
                ChatKey::ChooseMachine(_)
                | ChatKey::NextMachine
                | ChatKey::Terminal
                | ChatKey::Browser
                | ChatKey::Ignored => return,
            }
            cx.stop_propagation();
            return;
        }
        match pane_picker_key(
            choices(),
            self.selected,
            &key.key,
            key.modifiers.control || key.modifiers.alt || key.modifiers.platform,
        ) {
            PanePickerKey::Select(index) => {
                self.selected = index;
                cx.notify();
            }
            PanePickerKey::Choose(index) => self.activate(index, cx),
            PanePickerKey::Close => self.close(cx),
            PanePickerKey::Ignored => return,
        }
        cx.stop_propagation();
    }
}

impl ala::EventEmitter<ChooseFolder> for PanePicker {}

impl ala::EventEmitter<LeaveLanding> for PanePicker {}

impl Focusable for PanePicker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PanePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let writable = self.connection.read(cx).connected
            && !self.connection.read(cx).core.attached_read_only();
        let phone = super::settings::phone();
        let folders = self.folders(cx).filter(|_| !phone);
        let rows = if let Some(folders) = folders {
            self.chat_rows(&folders, writable, cx)
        } else {
            self.choice_rows(writable, phone, cx)
        };
        pane_picker_surface(("web-pane-picker", self.id()), self.corner_radii, rows, cx)
            .key_context("PanePicker")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.sheet_closed = false;
                    if let Some(pane) = this.pane {
                        this.connection.update(cx, |connection, cx| {
                            connection.command(
                                "select-pane",
                                vec!["-Z".into(), "-t".into(), pane.to_string()],
                                cx,
                            );
                        });
                    }
                    this.focus.focus(window, cx);
                }),
            )
    }
}
