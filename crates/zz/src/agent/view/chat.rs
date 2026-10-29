use std::rc::Rc;

use zz_gpui::{
    AnyElement, Context, Entity, IntoElement, ScrollWheelEvent, Window, div, prelude::*, px,
};
use zz_protocol::chat::{ChatView, RequestView};
use zz_ui::ActiveTheme as _;
use zz_ui::agent::chat::{ChatClicks, ChatHooks, chat_live, chat_thread, shows_chat};
use zz_ui::agent::presentation::spinner_phase;
use zz_ui::agent::{AGENT_CONTENT_MAX_WIDTH, agent_jump_to_bottom_button};

use super::{AgentPaneState, AgentView, COMPOSER_OUTER_PADDING};

impl AgentView {
    fn on_chat_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat.wheel(event) {
            cx.defer_in(window, |view, _, cx| {
                if view.chat.settle() {
                    cx.notify();
                }
            });
        }
    }

    pub(super) fn asks_out_of_view(&self) -> bool {
        self.chat_inline && self.chat.away
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

    fn inline_ask(
        &self,
        request: &RequestView,
        state: &AgentPaneState,
        view: &Entity<Self>,
        cx: &zz_gpui::App,
    ) -> Option<AnyElement> {
        let permission = state
            .pending_permissions
            .iter()
            .find(|permission| permission.request_id == request.id.0)?;
        self.render_permission(permission, state, view, cx)
    }

    pub(super) fn render_chat(
        &mut self,
        state: &AgentPaneState,
        view: &Entity<Self>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let live = self
            .controller
            .read(cx)
            .pane_chat(self.pane)
            .is_some_and(chat_live);
        let phase = if live {
            spinner_phase(cx.entity_id(), cx)
        } else {
            0.0
        };
        self.chat.pin();
        let click: ChatClicks = {
            let view = view.clone();
            Rc::new(move |click, _, cx| {
                view.update(cx, |view, cx| {
                    view.chat.click(click);
                    cx.notify();
                });
            })
        };
        let pane = self.pane.0;
        let wheel = cx.listener(Self::on_chat_wheel);
        let show_jump = self.chat.shows_jump();
        let jump = {
            let view = view.clone();
            move |_: &zz_gpui::ClickEvent, _: &mut Window, cx: &mut zz_gpui::App| {
                view.update(cx, |view, cx| {
                    view.chat.follow();
                    cx.notify();
                });
                cx.stop_propagation();
            }
        };
        let now = zz_client::chat_rows::now_ms();
        let app: &zz_gpui::App = cx;
        let Some(chat) = self.controller.read(app).pane_chat(self.pane) else {
            return div().into_any_element();
        };
        self.chat.sync(chat);
        let ask =
            |request: &RequestView, cx: &zz_gpui::App| self.inline_ask(request, state, view, cx);
        let hooks = ChatHooks {
            click: Some(click),
            ask: Some(&ask),
        };
        let body = chat_thread(chat, now, &self.chat.open, phase, &hooks, app);
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id(("agent-chat-scroll", pane))
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.chat.scroll)
                    .on_scroll_wheel(wheel)
                    .child(
                        div().w_full().px_3().child(
                            div()
                                .w_full()
                                .max_w(px(AGENT_CONTENT_MAX_WIDTH))
                                .mx_auto()
                                .pt_3()
                                .pb(px(COMPOSER_OUTER_PADDING + 12.0))
                                .text_color(app.theme().foreground)
                                .child(body),
                        ),
                    ),
            )
            .when(show_jump, |root| {
                root.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(px(2.0 * COMPOSER_OUTER_PADDING))
                        .flex()
                        .justify_center()
                        .child(
                            agent_jump_to_bottom_button(("agent-chat-jump", pane), app)
                                .on_click(jump),
                        ),
                )
            })
            .into_any_element()
    }
}
