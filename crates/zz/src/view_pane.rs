use std::rc::Rc;

use ala::{
    Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    div,
};
use penna::v_flex;
use zz_client::program::Program;
use zz_protocol::{CommandInvocation, PaneId, ViewDescriptor};
use zz_ui::{
    agent::{
        chat::{ChatClicks, ChatPane},
        view::{VIEW_SOURCE, ViewChat, view_body, view_buttons, view_chat_body},
    },
    pane::PaneKindButton,
};

use crate::{agent::AgentController, mux::client::MuxClient};

pub(crate) struct ViewPaneView {
    pane: PaneId,
    descriptor: ViewDescriptor,
    vendor: Option<&'static Program>,
    mux: Entity<MuxClient>,
    agents: Entity<AgentController>,
    chat: ChatPane,
    focus: FocusHandle,
    source: bool,
    _agents: Subscription,
}

impl ViewPaneView {
    pub(crate) fn new(
        pane: PaneId,
        descriptor: &ViewDescriptor,
        vendor: Option<&'static Program>,
        mux: Entity<MuxClient>,
        agents: Entity<AgentController>,
        cx: &mut Context<Self>,
    ) -> Self {
        let observed = cx.observe(&agents, |view, _, cx| {
            if view.descriptor.content.text().is_none() {
                cx.notify();
            }
        });
        Self {
            pane,
            descriptor: descriptor.clone(),
            vendor,
            mux,
            agents,
            chat: ChatPane::default(),
            focus: cx.focus_handle(),
            source: false,
            _agents: observed,
        }
    }

    pub(crate) fn synchronize_descriptor(
        &mut self,
        descriptor: &ViewDescriptor,
        vendor: Option<&'static Program>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.vendor = vendor;
        if self.descriptor == *descriptor {
            return false;
        }
        self.descriptor = descriptor.clone();
        self.source = false;
        cx.notify();
        true
    }

    pub(crate) fn focus(&self) -> FocusHandle {
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
        self.mux.read(cx).execute(CommandInvocation::new(
            "kill-pane",
            ["-t".to_owned(), self.pane.to_string()],
        ));
    }
}

impl Render for ViewPaneView {
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
            let agents = self.agents.clone();
            let controller = agents.read(cx);
            let chat = self
                .descriptor
                .origin
                .and_then(|origin| controller.pane_chat(origin));
            if let Some(chat) = chat {
                self.chat.sync(chat);
            }
            let live = chat.is_some_and(zz_ui::agent::chat::chat_live);
            let phase = if live {
                penna::blocks::chat::spinner_phase(entity.entity_id(), cx)
            } else {
                0.0
            };
            let chat = self
                .descriptor
                .origin
                .and_then(|origin| agents.read(cx).pane_chat(origin));
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
