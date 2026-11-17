use ala::{AnyElement, App, Context, Window};
use zz_protocol::{
    PaneId,
    chat::{ChatView, ItemId},
};
use zz_ui::agent::chat::{ChatHost, ChatLook, ChatPane, chat_surface};

use super::AgentView;

impl ChatHost for AgentView {
    fn chat_pane(&mut self) -> &mut ChatPane {
        &mut self.chat
    }

    fn chat_parts<'a>(&'a mut self, cx: &'a App) -> (&'a mut ChatPane, Option<&'a ChatView>) {
        (
            &mut self.chat,
            self.controller.read(cx).pane_chat(self.pane),
        )
    }

    fn chat_read<'a>(&'a self, cx: &'a App) -> (&'a ChatPane, Option<&'a ChatView>) {
        (&self.chat, self.controller.read(cx).pane_chat(self.pane))
    }

    fn chat_fork(&mut self, item: &ItemId, cx: &mut Context<Self>) {
        self.fork_from(item.as_str(), cx);
    }

    fn chat_open_child(&mut self, child: PaneId, cx: &mut Context<Self>) {
        let command = zz_client::child_card::reopen_command(self.pane, child);
        if let Some(request) = self.mux.read(cx).execute_printed(command) {
            self.child_requests.insert(request, child);
        }
    }
}

impl AgentView {
    pub(super) fn render_chat(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let look = ChatLook {
            pane: self.pane.0,
            fork: self.forks(),
            refs: Some(self.chat_refs(&view, cx)),
            vendor: zz_client::window_summary::provider_program(self.pane_state.provider),
            ground: Some(self.pane_ground(cx)),
            touch: false,
            inset_top: self.header_inset(),
            inset_bottom: ala::px(0.0),
            children: Some({
                let controller = self.controller.clone();
                zz_ui::agent::chat::child_chats(move |pane, cx| controller.read(cx).pane_chat(pane))
            }),
        };
        chat_surface(self, look, window, cx)
    }
}
