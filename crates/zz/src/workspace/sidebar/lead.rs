use zz_gpui::{
    AnyElement, AppContext as _, Context, DragMoveEvent, Entity, InteractiveElement as _,
    IntoElement as _, ParentElement as _, Pixels, ScrollHandle, SharedString, Styled as _, Window,
    div, px,
};
use zz_ui::{
    ActiveTheme as _,
    agent::{
        chat::{
            ChatComposer, ChatSection, ChatSplitDrag, chat_header, chat_split_height, chat_thread,
        },
        controls::{ComposerAction, composer_action_button},
    },
    input::{InputEvent, InputState},
    navigation::workspace_sidebar_fade,
    v_flex,
};

use super::WorkspaceSidebar;
use crate::{agent::lead::LeadChat, mux::client::MuxClient};

const PLACEHOLDER: &str = "Message the lead…";

/// The lead chat at the bottom of the sidebar, under a split you drag.
pub(super) struct LeadSection {
    chat: Entity<LeadChat>,
    input: Option<Entity<InputState>>,
    scroll: ScrollHandle,
    height: Option<Pixels>,
    shown: usize,
}

impl LeadSection {
    pub(super) fn new(
        mux: Entity<MuxClient>,
        local_name: SharedString,
        cx: &mut Context<WorkspaceSidebar>,
    ) -> Self {
        let chat = cx.new(|cx| LeadChat::new(mux, local_name, cx));
        cx.observe(&chat, |_, _, cx| cx.notify()).detach();
        Self {
            chat,
            input: None,
            scroll: ScrollHandle::new(),
            height: None,
            shown: 0,
        }
    }
}

impl WorkspaceSidebar {
    pub(crate) fn lead_chat(&self) -> Entity<LeadChat> {
        self.lead.chat.clone()
    }

    fn lead_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some(input) = &self.lead.input {
            return input.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        cx.subscribe_in(
            &input,
            window,
            |sidebar, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false } => sidebar.send_to_lead(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();
        self.lead.input = Some(input.clone());
        input
    }

    fn send_to_lead(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.lead.input.clone() else {
            return;
        };
        let text = input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        if self.lead.chat.update(cx, |chat, cx| chat.send(&text, cx)) {
            input.update(cx, |input, cx| input.set_value("", window, cx));
            self.lead.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// `navigation` over the lead chat while there is a lead; `navigation`
    /// alone otherwise.
    pub(super) fn with_lead_chat(
        &mut self,
        navigation: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat = self.lead.chat.clone();
        let Some((lead, host_name)) = chat
            .read(cx)
            .lead_sender()
            .zip(chat.read(cx).lead().map(|lead| lead.host_name.clone()))
        else {
            return navigation;
        };
        let input = self.lead_input(window, cx);
        let ground = self.content_ground(cx);
        let fade = workspace_sidebar_fade(self.ground(cx), cx);
        let ((bubbles, depth), running, error) = {
            let chat = chat.read(cx);
            (
                chat.thread("workspace-lead", ground, cx),
                chat.running(cx),
                chat.error().cloned(),
            )
        };
        if depth > self.lead.shown {
            self.lead.scroll.scroll_to_bottom();
        }
        self.lead.shown = depth;
        chat.update(cx, |chat, cx| chat.mark_read(cx));
        let has_content = !input.read(cx).value().trim().is_empty();
        let action = if running && has_content {
            ComposerAction::Queue
        } else {
            ComposerAction::Send
        };
        let sidebar = cx.entity();
        let send = composer_action_button("workspace-lead-send", action, has_content).on_click(
            move |_, window, cx| sidebar.update(cx, |sidebar, cx| sidebar.send_to_lead(window, cx)),
        );
        let composer = v_flex()
            .w_full()
            .children(
                LeadChat::ask(&chat, "workspace-lead-ask", cx)
                    .map(|ask| div().px(px(10.0)).child(ask)),
            )
            .children(error.map(|error| {
                div()
                    .px(px(18.0))
                    .pb(px(6.0))
                    .text_size(zz_ui::rems_from_px(12.0))
                    .text_color(cx.theme().danger)
                    .child(error.to_string())
            }))
            .child(ChatComposer {
                input,
                action: send.into_any_element(),
                ground: Some(ground),
            });
        div()
            .id("workspace-sidebar-column")
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .on_drag_move::<ChatSplitDrag>(cx.listener(
                |sidebar, event: &DragMoveEvent<ChatSplitDrag>, _, cx| {
                    sidebar.lead.height =
                        Some(chat_split_height(event.event.position.y, event.bounds));
                    cx.notify();
                },
            ))
            .child(div().flex().flex_col().flex_1().min_h_0().child(navigation))
            .child(
                ChatSection::new(
                    chat_header(&lead, host_name, None, cx),
                    chat_thread(bubbles),
                    composer,
                    &self.lead.scroll,
                    fade,
                )
                .height(self.lead.height),
            )
            .into_any_element()
    }
}
