//! Inert stand-ins compiled when the `agent-pane` feature is off.

// Stub methods keep the real API's `self` receivers without reading them.
#![allow(clippy::unused_self)]

use std::sync::Arc;

use zz_gpui::{App, Context, Task, Window, div, prelude::*};
use zz_protocol::{AgentProvider, PaneId};
use zz_ui::{ActiveTheme as _, Colorize as _};

use crate::mux::client::MuxClient;

#[derive(Default)]
pub struct AgentController;

impl AgentController {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn attach_mux(&mut self, _mux: zz_gpui::Entity<MuxClient>) {}

    pub(crate) fn retain_panes(
        &mut self,
        _retained: &std::collections::BTreeSet<PaneId>,
        _cx: &mut Context<Self>,
    ) {
    }

    pub(crate) fn prompt(
        &mut self,
        _pane: PaneId,
        _text: &str,
        _images: Vec<Arc<zz_gpui::Image>>,
        _cx: &mut Context<Self>,
    ) -> Result<(), Arc<str>> {
        Err("the agent lens is not included in this build".into())
    }

    pub(crate) fn is_shutting_down(&self) -> bool {
        false
    }

    pub(crate) fn is_shutdown_complete(&self) -> bool {
        true
    }

    pub(crate) fn shutdown(&mut self, _cx: &mut Context<Self>) -> Task<bool> {
        Task::ready(true)
    }
}

pub(crate) struct AgentView {
    focus_handle: zz_gpui::FocusHandle,
}

impl AgentView {
    pub(crate) fn new(
        _pane: PaneId,
        _provider: AgentProvider,
        _controller: zz_gpui::Entity<AgentController>,
        _mux: zz_gpui::Entity<MuxClient>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    pub(crate) fn focus(&self, _cx: &App) -> zz_gpui::FocusHandle {
        self.focus_handle.clone()
    }

    pub(crate) fn set_visible(&mut self, _visible: bool, _cx: &mut Context<Self>) {}

    pub(crate) fn set_window_corners(
        &mut self,
        _corners: crate::window::corners::WindowCorners,
        _cx: &mut Context<Self>,
    ) {
    }
}

impl Render for AgentView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().foreground.muted())
            .child("The agent lens is not included in this build")
    }
}
