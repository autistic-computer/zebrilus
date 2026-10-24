//! The command palette's Lead mode: the lead chat in the palette's surface,
//! opened by the title bar's chat button while the sidebar is hidden.

use zz_gpui::{
    AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ParentElement as _, Render,
    ScrollHandle, Styled as _, Window, div, px,
};
use zz_ui::{
    ActiveTheme as _, PaintState,
    agent::chat::{LEAD_PALETTE_HINTS, chat_thread, lead_palette_body},
    command::{CommandPaletteSurface, PalettePill, PaletteStatus, unified_command_palette_input},
    input::{InputEvent, InputState},
    roles,
};

use super::lead::LeadChat;

const PLACEHOLDER: &str = "Message the lead…";

pub(crate) enum LeadPaletteEvent {
    /// Esc or a click outside: close the palette.
    Close,
    /// Backspace in an empty field: back to the palette's other modes.
    LeaveMode,
}

pub(crate) struct LeadPalette {
    chat: Entity<LeadChat>,
    input: Entity<InputState>,
    scroll: ScrollHandle,
    shown: usize,
}

impl EventEmitter<LeadPaletteEvent> for LeadPalette {}

impl LeadPalette {
    pub(crate) fn new(chat: Entity<LeadChat>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        cx.subscribe_in(
            &input,
            window,
            |palette, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false } => palette.send(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();
        cx.observe(&chat, |_, _, cx| cx.notify()).detach();
        let scroll = ScrollHandle::new();
        scroll.scroll_to_bottom();
        Self {
            chat,
            input,
            scroll,
            shown: 0,
        }
    }

    pub(crate) fn focus(&self, cx: &zz_gpui::App) -> FocusHandle {
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

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let empty = self.input.read(cx).value().is_empty();
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.emit(LeadPaletteEvent::Close);
                cx.stop_propagation();
            }
            "backspace" if empty => {
                cx.emit(LeadPaletteEvent::LeaveMode);
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

impl Render for LeadPalette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let chat = self.chat.clone();
        let layer = cx.theme().layer(roles::PALETTE, PaintState::Rest);
        let (pills, bubbles) = {
            let chat = chat.read(cx);
            let host = chat.lead().map(|lead| PalettePill::Host {
                label: lead.host_name.clone(),
                status: PaletteStatus::Online,
            });
            let lead =
                chat.lead()
                    .and_then(|lead| lead.program)
                    .map(|program| PalettePill::Program {
                        label: "Lead".into(),
                        program,
                    });
            (
                host.into_iter().chain(lead).collect::<Vec<_>>(),
                chat.thread("lead-palette", layer.ground, cx),
            )
        };
        let (bubbles, depth) = bubbles;
        if depth > self.shown {
            self.scroll.scroll_to_bottom();
        }
        self.shown = depth;
        chat.update(cx, |chat, cx| chat.mark_read(cx));
        let ask = LeadChat::ask(&chat, "lead-palette-ask", cx);
        let surface = CommandPaletteSurface::new(
            unified_command_palette_input(&self.input, pills, cx),
            cx.entity_id().as_u64(),
        )
        .rows(lead_palette_body(
            chat_thread(bubbles),
            ask,
            &self.scroll,
            layer.ground.fill,
        ))
        .hints(LEAD_PALETTE_HINTS);
        let palette = cx.entity();
        div()
            .id("lead-palette-overlay")
            .debug_selector(|| "lead-palette-overlay".to_owned())
            .absolute()
            .inset_0()
            .size_full()
            .occlude()
            .flex()
            .items_start()
            .justify_center()
            .px(px(24.0))
            .pt(px(22.0))
            .capture_key_down(cx.listener(Self::on_key_down))
            .on_key_up(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                palette.update(cx, |_, cx| cx.emit(LeadPaletteEvent::Close));
                cx.stop_propagation();
            })
            .child(surface)
    }
}
