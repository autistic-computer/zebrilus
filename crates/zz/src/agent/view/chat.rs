use ala::{AnyElement, App, Context, Entity, Window};
use zz_protocol::chat::{ChatView, ItemId, RequestView};
use zz_ui::agent::chat::{ChatHost, ChatLook, ChatPane, chat_surface, shows_chat};

use super::{AgentPaneState, AgentView};

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

    fn chat_ask(&self, request: &RequestView, host: &Entity<Self>, cx: &App) -> Option<AnyElement> {
        let permission = self
            .pane_state
            .pending_permissions
            .iter()
            .find(|permission| permission.request_id == request.id.0)?;
        self.render_permission(permission, &self.pane_state, host, cx)
    }

    fn chat_fork(&mut self, item: &ItemId, cx: &mut Context<Self>) {
        self.fork_from(item.as_str(), cx);
    }
}

impl AgentView {
    pub(super) fn asks_out_of_view(&self) -> bool {
        self.chat_inline && self.chat.away()
    }

    pub(super) fn asks_inline(&self, chat: &ChatView, state: &AgentPaneState) -> bool {
        shows_chat(chat)
            && self
                .permission_wizard
                .current(&state.pending_permissions)
                .is_some_and(|head| {
                    chat.open_requests()
                        .any(|request| request.id.0 == head.request_id)
                })
    }

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
            vendor: Some(match self.pane_state.provider {
                zz_protocol::AgentProvider::ClaudeCode => zz_ui::program::CLAUDE_ICON,
                zz_protocol::AgentProvider::Codex => zz_ui::program::OPENAI_ICON,
            }),
        };
        chat_surface(self, look, window, cx)
    }
}
