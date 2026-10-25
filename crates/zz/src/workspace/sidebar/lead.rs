use zz_gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use zz_ui::agent::chat::{ChatBubble, ChatTyping, chat_header, chat_latest};

use super::{SidebarOpenLead, WorkspaceSidebar};
use crate::{agent::lead::LeadChat, mux::client::MuxClient};

/// How many lines of the lead's last answer the sidebar shows.
const LATEST_LINES: usize = 4;
const ATTENTION_DOT: f32 = 7.0;

/// The lead's last message at the bottom of the sidebar. The chat itself is
/// the command palette's Lead mode, which a click on it opens.
pub(super) struct LeadSection {
    chat: Entity<LeadChat>,
}

impl LeadSection {
    pub(super) fn new(
        mux: Entity<MuxClient>,
        local_name: SharedString,
        cx: &mut Context<WorkspaceSidebar>,
    ) -> Self {
        let chat = cx.new(|cx| LeadChat::new(mux, local_name, cx));
        cx.observe(&chat, |_, _, cx| cx.notify()).detach();
        Self { chat }
    }
}

impl WorkspaceSidebar {
    pub(crate) fn lead_chat(&self) -> Entity<LeadChat> {
        self.lead.chat.clone()
    }

    /// `navigation` over the lead's last message while there is a lead and it
    /// has said something or is at work; `navigation` alone otherwise.
    pub(super) fn with_lead_latest(
        &self,
        navigation: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat = self.lead.chat.read(cx);
        let (Some(sender), Some(lead)) = (chat.lead_sender(), chat.lead()) else {
            return navigation;
        };
        let ground = self.content_ground(cx);
        let message = if let Some(steps) = chat.steps(cx) {
            ChatTyping::new("workspace-lead-typing")
                .steps(steps)
                .on(ground)
                .into_any_element()
        } else if let Some(text) = chat.last_answer(cx) {
            ChatBubble::theirs("workspace-lead-latest-message", text, "")
                .lines(LATEST_LINES)
                .on(ground)
                .into_any_element()
        } else {
            return navigation;
        };
        let attention = chat.attention(cx).map(|color| {
            div()
                .size(px(ATTENTION_DOT))
                .rounded_full()
                .bg(color)
                .into_any_element()
        });
        let header = chat_header(&sender, lead.host_name.clone(), attention, cx);
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(div().flex().flex_col().flex_1().min_h_0().child(navigation))
            .child(
                chat_latest("workspace-lead-latest", header, message, cx)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SidebarOpenLead))),
            )
            .into_any_element()
    }
}
