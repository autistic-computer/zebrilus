//! The command palette's Driver mode: the driver chat in the palette's surface,
//! opened by the title bar's chat button while the sidebar is hidden.

use zz_gpui::{
    AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ParentElement as _, Render,
    ScrollHandle, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use zz_ui::{
    ActiveTheme as _, PaintState,
    agent::{
        chat::{
            CHAT_COMPOSER_INSET, chat_thread, driver_palette_body, driver_palette_hints,
            driver_palette_place, driver_palette_thread_least,
        },
        composer::{Composer, composer_ground},
        controls::{
            ComposerAction, composer_action_button, composer_add_button, context_usage_meter,
        },
    },
    command::{COMMAND_PALETTE_INSET, CommandPaletteSurface, PaletteStatus, target_picker},
    input::{InputEvent, InputState},
    link::{MentionLink, OpenLink},
    roles,
    target::{TargetPicker, mention_at_caret, mention_keys},
};

use super::driver::DriverChat;

const PLACEHOLDER: &str = "Message the driver…";
/// How much of the window the palette may take in Driver mode, below its
/// top margin, like the other modes.
const THREAD_SHARE: f32 = 0.88;
/// The palette's input, footer and bottom margin, which the thread leaves
/// room for.
const CHROME_HEIGHT: f32 = 152.0;
const THREAD_MIN: f32 = 240.0;
/// Room an approval card under the thread takes.
const ASK_HEIGHT: f32 = 170.0;

pub(crate) enum DriverPaletteEvent {
    /// Esc or a click outside: close the palette.
    Close,
    /// Backspace in an empty field: back to the palette's other modes.
    LeaveMode,
    /// The footer's host label: show the driver's terminal.
    ShowTerminal,
    /// A zz link in the thread: go to what it names.
    Open(zz_gpui::SharedString),
}

pub(crate) struct DriverPalette {
    chat: Entity<DriverChat>,
    input: Entity<InputState>,
    scroll: ScrollHandle,
    shown: usize,
    /// The `@` picker over the input.
    mention: Entity<TargetPicker>,
}

impl EventEmitter<DriverPaletteEvent> for DriverPalette {}

impl DriverPalette {
    pub(crate) fn new(
        chat: Entity<DriverChat>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        cx.subscribe_in(
            &input,
            window,
            |palette, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false } => palette.send(false, window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();
        cx.observe_in(&chat, window, |palette, _, window, cx| {
            palette.refill(window, cx);
            cx.notify();
        })
        .detach();
        let scroll = ScrollHandle::new();
        scroll.scroll_to_bottom();
        let backend = crate::command::palette::palette_backend(chat.read(cx).mux().clone());
        let mention = target_picker(&backend, Some(input.clone()), cx);
        Self {
            chat,
            input,
            scroll,
            shown: 0,
            mention,
        }
    }

    pub(crate) fn focus(&self, cx: &zz_gpui::App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }

    /// Send the input. While the driver works it steers the running turn, or
    /// with `queue` waits for the turn to end.
    fn send(&mut self, queue: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let sent = self.chat.update(cx, |chat, cx| {
            if queue {
                chat.send(&text, cx)
            } else {
                chat.steer(&text, cx)
            }
        });
        if sent {
            self.input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// Put messages the daemon handed back from the queue into the input.
    fn refill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.chat.update(cx, DriverChat::take_reclaimed) else {
            return;
        };
        self.input.update(cx, |input, cx| {
            let value = input.value();
            let text = if value.trim().is_empty() {
                text
            } else {
                format!("{text}\n{value}")
            };
            input.set_value(text, window, cx);
        });
    }

    /// The `+` at the composer's start: it starts a mention at the caret.
    fn mention_button(&self, cx: &zz_gpui::App) -> zz_gpui::AnyElement {
        let input = self.input.clone();
        composer_add_button("driver-mention", "Mention a host, session or file")
            .on(composer_ground(cx))
            .on_click(move |_, window, cx| mention_at_caret(&input, "@", window, cx))
            .into_any_element()
    }

    fn on_mention_link(&mut self, link: &MentionLink, window: &mut Window, cx: &mut Context<Self>) {
        mention_at_caret(&self.input, &format!("{} ", link.address), window, cx);
    }

    /// The round button at the composer's end: send, or steer while the
    /// driver works, and stop while it works and the field is empty.
    fn action_button(&self, running: bool, cx: &Context<Self>) -> zz_gpui::AnyElement {
        let typed = !self.input.read(cx).value().trim().is_empty();
        let stop = running && !typed;
        let palette = cx.entity();
        composer_action_button(
            "driver-send",
            if stop {
                ComposerAction::Stop
            } else {
                ComposerAction::Send
            },
            stop || typed,
        )
        .on(composer_ground(cx))
        .on_click(move |_, window, cx| {
            palette.update(cx, |palette, cx| {
                if stop {
                    palette.chat.update(cx, DriverChat::stop);
                } else {
                    palette.send(false, window, cx);
                }
            });
        })
        .into_any_element()
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let empty = self.input.read(cx).value().is_empty();
        let modifiers = event.keystroke.modifiers;
        if modifiers.control
            && event.keystroke.key == "c"
            && self.input.read(cx).selected_range().is_empty()
            && self.chat.update(cx, DriverChat::stop)
        {
            cx.stop_propagation();
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.emit(DriverPaletteEvent::Close);
                cx.stop_propagation();
            }
            "backspace" if empty => {
                cx.emit(DriverPaletteEvent::LeaveMode);
                cx.stop_propagation();
            }
            "tab"
                if !empty
                    && modifiers.number_of_modifiers() == 0
                    && self.chat.read(cx).running(cx) =>
            {
                self.send(true, window, cx);
                cx.stop_propagation();
            }
            "up" if empty && modifiers.number_of_modifiers() == 0 => {
                if let Some(text) = self.chat.update(cx, DriverChat::take_back) {
                    self.input
                        .update(cx, |input, cx| input.set_value(text, window, cx));
                }
                cx.stop_propagation();
            }
            digit if empty && event.keystroke.modifiers.number_of_modifiers() == 0 => {
                let Some(index) = digit
                    .parse::<usize>()
                    .ok()
                    .and_then(|digit| digit.checked_sub(1))
                else {
                    return;
                };
                if self
                    .chat
                    .update(cx, |chat, cx| chat.answer_waiting(index, cx))
                {
                    cx.stop_propagation();
                }
            }
            _ => {}
        }
    }
}

impl Render for DriverPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let chat = self.chat.clone();
        let layer = cx.theme().layer(roles::PALETTE, PaintState::Rest);
        let (place, bubbles) = {
            let chat = chat.read(cx);
            (
                chat.driver()
                    .map(|driver| (driver.host_name.clone(), driver.program)),
                chat.thread("driver-palette", layer.ground, cx),
            )
        };
        let (bubbles, depth) = bubbles;
        if depth > self.shown {
            self.scroll.scroll_to_bottom();
        }
        self.shown = depth;
        chat.update(cx, |chat, cx| chat.mark_read(cx));
        let ask = DriverChat::ask(&chat, "driver-palette-ask", cx).or_else(|| {
            chat.read(cx).error().map(|error| {
                div()
                    .px(px(14.0))
                    .pb(px(8.0))
                    .text_size(zz_ui::rems_from_px(12.0))
                    .text_color(cx.theme().danger)
                    .child(error.to_string())
                    .into_any_element()
            })
        });
        let running = chat.read(cx).running(cx);
        let hints = driver_palette_hints(running, ask.is_some());
        let chrome = CHROME_HEIGHT + if ask.is_some() { ASK_HEIGHT } else { 0.0 };
        let viewport = window.viewport_size().height;
        let thread_height = (viewport * THREAD_SHARE - px(chrome)).max(px(THREAD_MIN));
        let thread_least = driver_palette_thread_least(viewport);
        self.mention.update(cx, |mention, _| {
            mention.fit(thread_least.min(thread_height) + px(COMMAND_PALETTE_INSET));
        });
        let usage = chat.read(cx).usage(cx).map(|(used, size)| {
            context_usage_meter("driver-context-usage", used, size, cx).into_any_element()
        });
        let composer = div()
            .flex_none()
            .w_full()
            .px(px(CHAT_COMPOSER_INSET))
            .pb(px(CHAT_COMPOSER_INSET))
            .child(Composer {
                input: self.input.clone(),
                leading: Some(self.mention_button(cx)),
                usage,
                action: self.action_button(running, cx),
                attachments: None,
                command_hint: None,
                ground: Some(layer.ground),
            });
        let surface = CommandPaletteSurface::new(composer, cx.entity_id().as_u64())
            .input_last()
            .rows(driver_palette_body(
                chat_thread(bubbles),
                ask,
                &self.scroll,
                layer.ground.fill,
                thread_least,
                thread_height,
            ))
            .hints(hints)
            .when_some(place, |surface, (host, program)| {
                let palette = cx.entity();
                surface.footer_leading(driver_palette_place(
                    host,
                    PaletteStatus::Online.color(cx),
                    program,
                    move |_, _, cx| {
                        palette.update(cx, |_, cx| cx.emit(DriverPaletteEvent::ShowTerminal));
                    },
                    cx,
                ))
            })
            .when(self.mention.read(cx).is_open(), |surface| {
                surface.over_input(self.mention.clone())
            });
        let palette = cx.entity();
        div()
            .id("driver-palette-overlay")
            .debug_selector(|| "driver-palette-overlay".to_owned())
            .absolute()
            .inset_0()
            .size_full()
            .occlude()
            .flex()
            .items_start()
            .justify_center()
            .px(px(16.0))
            .pt(window.viewport_size().height * 0.12)
            .capture_key_down(cx.listener(Self::on_key_down))
            .on_action(cx.listener(|_, link: &OpenLink, _, cx| {
                cx.emit(DriverPaletteEvent::Open(link.address.clone()));
            }))
            .on_action(cx.listener(Self::on_mention_link))
            .map(|overlay| mention_keys(overlay, &self.mention))
            .on_key_up(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                palette.update(cx, |_, cx| cx.emit(DriverPaletteEvent::Close));
                cx.stop_propagation();
            })
            .child(surface)
    }
}
