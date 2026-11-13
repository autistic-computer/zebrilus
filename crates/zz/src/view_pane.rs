use std::{rc::Rc, sync::Arc};

use ala::{
    App, Context, Entity, FocusHandle, Image, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    div,
};
use penna::v_flex;
use zz_client::program::Program;
use zz_protocol::{CommandInvocation, PaneId, ViewDescriptor};
use zz_ui::{
    agent::{
        chat::{ChatClicks, ChatPane},
        view::{
            VIEW_SOURCE, ViewChat, composer_image_id, view_body, view_buttons, view_chat_body,
            view_image, view_missing,
        },
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
    held: Option<u64>,
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
        cx.on_release(|view, cx| {
            if let Some(id) = view.held.take() {
                view.agents.update(cx, |agents, _| agents.release_image(id));
            }
        })
        .detach();
        let mut view = Self {
            pane,
            descriptor: descriptor.clone(),
            vendor,
            mux,
            agents,
            chat: ChatPane::default(),
            focus: cx.focus_handle(),
            source: false,
            held: None,
            _agents: observed,
        };
        view.hold(cx);
        view
    }

    fn hold(&mut self, cx: &mut Context<Self>) {
        let id = composer_image_id(&self.descriptor.content);
        if id == self.held {
            return;
        }
        let held = self.held.take();
        self.held = self.agents.update(cx, |agents, _| {
            if let Some(held) = held {
                agents.release_image(held);
            }
            id.filter(|id| agents.hold_image(*id))
        });
    }

    fn composer_image(&self, cx: &App) -> Option<Arc<Image>> {
        composer_image_id(&self.descriptor.content)
            .and_then(|id| self.agents.read(cx).shown_image(id))
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
        self.hold(cx);
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
        let body = if composer_image_id(&self.descriptor.content).is_some() {
            self.composer_image(cx).map_or_else(
                || view_missing("This image is no longer in the chat", cx),
                view_image,
            )
        } else if self.descriptor.content.text().is_some() {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ala::{AppContext as _, Image, ImageFormat, TestAppContext};
    use zz_daemon_client::DaemonError;
    use zz_protocol::{PaneId, ViewDescriptor};
    use zz_ui::agent::view::composer_image_view;

    use super::ViewPaneView;
    use crate::{agent::AgentController, mux::client::MuxClient};

    #[ala::test]
    fn a_composer_image_shows_in_its_view_pane_until_the_last_one_closes(cx: &mut TestAppContext) {
        let image = Arc::new(Image::from_bytes(ImageFormat::Png, vec![7, 7, 7]));
        let id = image.id;
        let agents = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        agents.update(cx, |agents, _| agents.show_image(Arc::clone(&image)));
        let descriptor = ViewDescriptor {
            origin: Some(PaneId(1)),
            title: "Image".to_owned(),
            content: composer_image_view(&image),
        };
        let view = |cx: &mut TestAppContext| {
            let (mux, agents) = (mux.clone(), agents.clone());
            cx.new(|cx| ViewPaneView::new(PaneId(2), &descriptor, None, mux, agents, cx))
        };
        let first = view(cx);
        let second = view(cx);
        let shown = cx.read(|cx| first.read(cx).composer_image(cx));
        assert!(shown.is_some_and(|shown| Arc::ptr_eq(&shown, &image)));
        drop(first);
        cx.update(|_| {});
        assert!(cx.read(|cx| second.read(cx).composer_image(cx)).is_some());
        drop(second);
        cx.update(|_| {});
        assert!(cx.read(|cx| agents.read(cx).shown_image(id)).is_none());
    }
}
