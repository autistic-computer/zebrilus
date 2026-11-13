use ala::{AnyElement, App, Context, Window};
use zz_protocol::chat::{ChatView, ItemId};
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
        };
        chat_surface(self, look, window, cx)
    }
}
