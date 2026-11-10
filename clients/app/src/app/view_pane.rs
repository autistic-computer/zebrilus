use std::rc::Rc;

use ala::{
    Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use penna::v_flex;
use zz_client::program::Program;
use zz_protocol::{CommandInvocation, PaneId, ViewDescriptor};
use zz_ui::{
    agent::{
        chat::{ChatClicks, ChatHost as _, ChatPane, chat_live},
        view::{VIEW_SOURCE, ViewChat, view_body, view_buttons, view_chat_body},
    },
    pane::PaneKindButton,
};

use super::agent_pane::AgentPane;
use crate::connection::Connection;

pub(crate) struct ViewPane {
    pane: PaneId,
    descriptor: ViewDescriptor,
    vendor: Option<&'static Program>,
    origin: Option<Entity<AgentPane>>,
    connection: Entity<Connection>,
    chat: ChatPane,
    focus: FocusHandle,
    source: bool,
}

impl ViewPane {
    pub(crate) fn new(
        pane: PaneId,
        descriptor: &ViewDescriptor,
        connection: Entity<Connection>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            pane,
            descriptor: descriptor.clone(),
            vendor: None,
            origin: None,
            connection,
            chat: ChatPane::default(),
            focus: cx.focus_handle(),
            source: false,
        }
    }

    pub(crate) fn synchronize(
        &mut self,
        descriptor: &ViewDescriptor,
        vendor: Option<&'static Program>,
        origin: Option<Entity<AgentPane>>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.vendor = vendor;
        self.origin = origin;
        if self.descriptor == *descriptor {
            return false;
        }
        self.descriptor = descriptor.clone();
        self.source = false;
        cx.notify();
        true
    }

    pub(crate) fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub(crate) fn header_buttons(&self) -> Vec<PaneKindButton> {
        view_buttons(&self.descriptor.content, self.source)
    }

    pub(crate) fn header_action(&mut self, key: &str, cx: &mut Context<Self>) {
        if key == VIEW_SOURCE {
            self.source = !self.source;
            cx.notify();
        }
    }

    fn close(&self, cx: &mut Context<Self>) {
        let close = CommandInvocation::new("kill-pane", ["-t".to_owned(), self.pane.to_string()]);
        self.connection
            .update(cx, |connection, cx| connection.invoke(close, cx));
    }
}

impl Render for ViewPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let body = if self.descriptor.content.text().is_some() {
            view_body(
                &self.descriptor.content,
                self.pane.0,
                self.source,
                window,
                cx,
            )
        } else {
            let click: ChatClicks = {
                let entity = entity.clone();
                Rc::new(move |click, _, cx| {
                    entity.update(cx, |view, cx| {
                        view.chat.click(click);
                        cx.notify();
                    });
                })
            };
            let origin = self.origin.clone();
            let live = origin.as_ref().is_some_and(|origin| {
                let pane = origin.read(cx);
                let chat = pane.chat_read(cx).1;
                if let Some(chat) = chat {
                    self.chat.sync(chat);
                }
                chat.is_some_and(chat_live)
            });
            let phase = if live {
                penna::blocks::chat::spinner_phase(entity.entity_id(), cx)
            } else {
                0.0
            };
            let chat = origin
                .as_ref()
                .and_then(|origin| origin.read(cx).chat_read(cx).1);
            view_chat_body(
                &self.descriptor.content,
                &ViewChat {
                    view: chat,
                    open: &self.chat.open,
                    now: zz_client::chat_rows::now_ms(),
                    phase,
                    click: Some(click),
                    vendor: self.vendor,
                    owner: self.pane.0,
                },
                cx,
            )
        };
        let closing = entity.downgrade();
        v_flex()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    closing.update(cx, |view, cx| view.close(cx)).ok();
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .id(("view-pane-body", self.pane.0))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}
