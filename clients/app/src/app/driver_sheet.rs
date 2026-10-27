use std::sync::Arc;

use zz_client::{
    agent_transcript::AgentPermissionKind,
    driver_chat::{DRIVER_SESSION, DriverMessageKind},
};
use zz_gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable as _,
    IntoElement, ParentElement as _, Render, ScrollHandle, SharedString, Styled as _, Window, div,
    prelude::*, px,
};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Emphasis, Tone,
    agent::{
        chat::{
            CHAT_COMPOSER_INSET, ChatBubble, ChatSender, ChatTyping, Receipt, chat_thread,
            phone_chat_place,
        },
        composer::Composer,
        controls::{
            ComposerAction, composer_action_button, composer_add_button, context_usage_meter,
        },
        presentation::{permission_card, permission_option},
    },
    button::Button,
    command::{PaletteSettings, target_hosts, target_picker},
    compact::{BottomSheet, bottom_sheet, close_sheet, sheet_close},
    input::{InputEvent, InputState},
    link::MentionLink,
    roles,
    scroll::ScrollableElement as _,
    target::{TargetHost, TargetPicker, mention_at_caret, mention_keys},
    v_flex,
};

use super::{
    AppShell,
    driver_chat::{DriverChat, Wake},
};
use crate::{command_palette::palette_backend, connection::Connection};

pub(super) const DRIVER_SHEET: &str = "driver-sheet";
const PLACEHOLDER: &str = "Message the driver…";
const THREAD_FADE: f32 = 24.0;
const SHEET_GAP: f32 = 8.0;

pub(super) enum DriverSheetEvent {
    Close,
    ShowTerminal,
}

pub(super) struct DriverSheet {
    chat: Entity<DriverChat>,
    input: Entity<InputState>,
    scroll: ScrollHandle,
    shown: usize,
    mention: Entity<TargetPicker>,
    hosts: Arc<[TargetHost]>,
    attached: Entity<Connection>,
}

impl EventEmitter<DriverSheetEvent> for DriverSheet {}

impl DriverSheet {
    pub(super) fn new(
        chat: Entity<DriverChat>,
        attached: Entity<Connection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(PLACEHOLDER)
                .auto_grow(1, 6)
                .submit_on_enter(true)
        });
        cx.subscribe_in(
            &input,
            window,
            |sheet, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => sheet.send(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();
        cx.observe(&chat, |_, _, cx| cx.notify()).detach();
        let scroll = ScrollHandle::new();
        scroll.scroll_to_bottom();
        let connection = chat
            .read(cx)
            .driver()
            .map_or_else(|| attached.clone(), |driver| driver.connection.clone());
        let backend = palette_backend(connection, PaletteSettings::default());
        let hosts: Arc<[TargetHost]> = target_hosts(&backend.tree(cx)).into();
        let mention = target_picker(&backend, Some(input.clone()), cx);
        Self {
            chat,
            input,
            scroll,
            shown: 0,
            mention,
            hosts,
            attached,
        }
    }

    fn asleep(&self, cx: &Context<Self>) -> AnyElement {
        let wake = self.chat.read(cx).wake_state();
        let (note, label, force) = match wake {
            Wake::Stalled => (
                "No driver started. When too few of your hosts answer, zz holds off so two drivers never run at once.",
                "Start here",
                true,
            ),
            Wake::Asleep | Wake::Waking => (
                "The Driver Agent is asleep. Wake follows a driver another host runs, or starts one on this host.",
                if wake == Wake::Waking {
                    "Waking"
                } else {
                    "Wake"
                },
                false,
            ),
        };
        let chat = self.chat.clone();
        let attached = self.attached.clone();
        let waking = wake == Wake::Waking;
        v_flex()
            .absolute()
            .inset_0()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .px(px(32.0))
            .child(
                div()
                    .text_center()
                    .text_color(cx.theme().foreground.muted())
                    .child(note),
            )
            .child(
                Button::new("driver-sheet-wake")
                    .tone(Tone::Primary)
                    .emphasis(Emphasis::Solid)
                    .label(label)
                    .loading(waking)
                    .on_click(move |_, _, cx| {
                        if !waking {
                            chat.update(cx, |chat, cx| chat.wake(&attached, force, cx));
                        }
                    }),
            )
            .into_any_element()
    }

    pub(super) fn focus(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        if self.chat.update(cx, |chat, cx| chat.send(&text, cx)) {
            self.input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    fn bubbles(&self, cx: &App) -> (Vec<AnyElement>, usize) {
        let ground = cx.theme().ground();
        let chat = self.chat.read(cx);
        let mut bubbles: Vec<AnyElement> = chat
            .messages(cx)
            .into_iter()
            .map(|message| {
                let id = (
                    "driver-sheet",
                    usize::try_from(message.id).unwrap_or(usize::MAX),
                );
                let time = chat.time(message.id);
                match message.kind {
                    DriverMessageKind::Mine { read } => ChatBubble::mine(id, message.text, time)
                        .receipt(if read {
                            Receipt::Read
                        } else {
                            Receipt::Delivered
                        })
                        .links(self.hosts.clone())
                        .on(ground)
                        .into_any_element(),
                    DriverMessageKind::Driver => ChatBubble::theirs(id, message.text, time)
                        .links(self.hosts.clone())
                        .on(ground)
                        .into_any_element(),
                    DriverMessageKind::Agent(pane) => {
                        let (name, program) = chat.sender_name(pane, cx);
                        ChatBubble::theirs(id, message.text, time)
                            .sender(ChatSender {
                                name: name.into(),
                                program,
                            })
                            .links(self.hosts.clone())
                            .on(ground)
                            .into_any_element()
                    }
                }
            })
            .collect();
        let mut depth = bubbles.len() * (zz_ui::agent::chat::TYPING_STEPS + 2);
        if let Some(steps) = chat.steps(cx) {
            depth += 1 + steps.len().min(zz_ui::agent::chat::TYPING_STEPS);
            bubbles.push(
                ChatTyping::new(("driver-sheet", usize::MAX))
                    .steps(steps)
                    .on(ground)
                    .into_any_element(),
            );
        }
        (bubbles, depth)
    }

    fn ask(&self, cx: &App) -> Option<AnyElement> {
        let request = self.chat.read(cx).permission()?;
        let options = request
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let chat = self.chat.clone();
                let option_id = option.id.clone();
                let request_id = request.request_id;
                permission_option(
                    ("driver-sheet-option", index),
                    index,
                    option.name.clone(),
                    matches!(
                        option.kind,
                        AgentPermissionKind::RejectOnce | AgentPermissionKind::RejectAlways
                    ),
                    index == 0,
                    true,
                    cx,
                )
                .on_click(move |_, _, cx| {
                    let option_id = option_id.clone();
                    chat.update(cx, |chat, cx| chat.respond(request_id, Some(option_id), cx));
                })
                .into_any_element()
            })
            .collect();
        let cancel = {
            let chat = self.chat.clone();
            let request_id = request.request_id;
            Button::new("driver-sheet-cancel")
                .tooltip("Cancel request")
                .on_click(move |_, _, cx| {
                    chat.update(cx, |chat, cx| chat.respond(request_id, None, cx));
                })
        };
        Some(
            div()
                .px(px(12.0))
                .pb(px(8.0))
                .child(permission_card(request.title, None, options, cancel, cx))
                .into_any_element(),
        )
    }

    fn action_button(&self, running: bool, cx: &Context<Self>) -> AnyElement {
        let typed = !self.input.read(cx).value().trim().is_empty();
        let stop = running && !typed;
        let sheet = cx.entity();
        composer_action_button(
            "driver-sheet-send",
            if stop {
                ComposerAction::Stop
            } else {
                ComposerAction::Send
            },
            stop || typed,
            cx,
        )
        .on_click(move |_, window, cx| {
            sheet.update(cx, |sheet, cx| {
                if stop {
                    sheet.chat.update(cx, DriverChat::stop);
                } else {
                    sheet.send(window, cx);
                }
            });
        })
        .into_any_element()
    }
}

impl Render for DriverSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (bubbles, depth) = self.bubbles(cx);
        if depth > self.shown {
            self.scroll.scroll_to_bottom();
        }
        self.shown = depth;
        self.chat.update(cx, |chat, cx| chat.mark_read(cx));
        let ground = cx.theme().ground_of(roles::DRIVER_DRAWER);
        let running = self.chat.read(cx).running(cx);
        let usage = self.chat.read(cx).usage().map(|(used, size)| {
            context_usage_meter("driver-sheet-usage", used, size, cx).into_any_element()
        });
        let ask = self.ask(cx);
        let input = self.input.clone();
        let leading = composer_add_button(
            "driver-sheet-mention",
            "Mention a host, session or file",
            cx,
        )
        .on_click(move |_, window, cx| mention_at_caret(&input, "@", window, cx))
        .into_any_element();
        let composer = Composer {
            input: self.input.clone(),
            leading: Some(leading),
            usage,
            action: self.action_button(running, cx),
            attachments: None,
            command_hint: None,
            ground: Some(ground),
        };
        let place = self.chat.read(cx).driver().map(|driver| {
            phone_chat_place(
                driver.host.clone(),
                driver.program,
                cx.listener(|_, _, _, cx| cx.emit(DriverSheetEvent::ShowTerminal)),
                cx,
            )
        });
        let viewport = window.viewport_size().height;
        let visible = window.fully_visible_bounds();
        let safe_bottom = (viewport - visible.bottom()).max(px(0.0));
        let keyboard = (viewport - window.visual_viewport_bounds().bottom()).max(px(0.0));
        let bottom = if keyboard > px(0.0) {
            keyboard + px(SHEET_GAP)
        } else {
            safe_bottom
        };
        let asleep = self
            .chat
            .read(cx)
            .driver()
            .is_none()
            .then(|| self.asleep(cx));
        let thread = div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id("driver-sheet-thread")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        v_flex()
                            .min_h_full()
                            .justify_end()
                            .child(chat_thread(bubbles)),
                    ),
            )
            .top_fade(&self.scroll, ground.fill, px(THREAD_FADE))
            .children(asleep);
        let body = v_flex()
            .debug_selector(|| "driver-sheet".to_owned())
            .flex_1()
            .min_h_0()
            .on_action(cx.listener(|sheet, link: &MentionLink, window, cx| {
                mention_at_caret(&sheet.input, &format!("{} ", link.address), window, cx);
            }))
            .map(|body| mention_keys(body, &self.mention))
            .child(thread)
            .children(ask)
            .children(
                self.mention
                    .read(cx)
                    .is_open()
                    .then(|| div().px(px(12.0)).child(self.mention.clone())),
            )
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .px(px(CHAT_COMPOSER_INSET))
                    .pb(px(CHAT_COMPOSER_INSET))
                    .child(composer),
            );
        let close =
            sheet_close("driver-sheet-close").on_click(|_, _, cx| close_sheet(DRIVER_SHEET, cx));
        let sheet = cx.weak_entity();
        bottom_sheet(
            DRIVER_SHEET,
            "Driver",
            [close.into_any_element()],
            body,
            bottom,
            move |_, cx| {
                let _ = sheet.update(cx, |_, cx| cx.emit(DriverSheetEvent::Close));
            },
        )
        .surface_role(roles::DRIVER_DRAWER)
        .header_role(roles::PHONE_BAR)
        .content_scroll(self.scroll.clone())
        .full(visible.top() + px(SHEET_GAP))
        .when_some(place, BottomSheet::heading)
    }
}

impl AppShell {
    pub(super) fn sync_driver_hosts(&mut self, cx: &mut Context<Self>) {
        let hosts = self
            .host_pages(cx)
            .into_iter()
            .filter_map(|page| Some((page.connection?, SharedString::from(page.title))))
            .collect();
        self.driver_chat
            .update(cx, |chat, cx| chat.set_hosts(hosts, cx));
    }

    pub(super) fn open_driver_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.driver_sheet.is_some() {
            return;
        }
        self.sync_driver_hosts(cx);
        let chat = self.driver_chat.clone();
        let attached = self.connection.clone();
        let sheet = cx.new(|cx| DriverSheet::new(chat, attached, window, cx));
        cx.subscribe_in(
            &sheet,
            window,
            |this, _, event: &DriverSheetEvent, window, cx| match event {
                DriverSheetEvent::Close => this.close_driver_chat(window, cx),
                DriverSheetEvent::ShowTerminal => this.show_driver_terminal(window, cx),
            },
        )
        .detach();
        if !Self::narrow(window) {
            sheet.read(cx).focus(cx).focus(window, cx);
        }
        self.driver_sheet = Some(sheet);
        cx.notify();
    }

    pub(super) fn open_driver_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_driver_chat(window, cx);
        if let Some(sheet) = &self.driver_sheet {
            sheet.read(cx).focus(cx).focus(window, cx);
            window.request_virtual_keyboard();
        }
    }

    pub(super) fn close_driver_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.driver_sheet.take().is_some() {
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    fn show_driver_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(driver) = self.driver_chat.read(cx).driver().cloned() else {
            return;
        };
        let session = driver
            .connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .iter()
            .find(|session| session.name == DRIVER_SESSION)
            .map(|session| session.id);
        self.close_driver_chat(window, cx);
        self.close_overview(cx);
        #[cfg(target_os = "ios")]
        if let Some(endpoint) = driver.connection.read(cx).endpoint().map(ToOwned::to_owned) {
            self.switch_host(&endpoint, session, Some(driver.pane), window, cx);
        }
        #[cfg(not(target_os = "ios"))]
        if let Some(session) = session {
            self.open_pane(session, driver.pane, cx);
        }
    }

    pub(super) fn driver_sheet_layer(&self, cx: &App) -> Option<AnyElement> {
        self.driver_sheet
            .clone()
            .map(|sheet| zz_ui::touch::touch_scale(sheet, cx))
    }
}
