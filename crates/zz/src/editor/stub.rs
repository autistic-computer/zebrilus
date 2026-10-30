#![allow(clippy::unused_self)]

use ala::{App, Context, Window, div, prelude::*};
use penna::{ActiveTheme as _, Colorize as _};
use zz_protocol::{EditorDescriptor, PaneId};

use crate::mux::client::MuxClient;

pub fn init(_cx: &mut App) {}

pub(crate) struct EditorView {
    focus_handle: ala::FocusHandle,
}

impl EditorView {
    pub(crate) fn new(
        _pane: PaneId,
        _descriptor: &EditorDescriptor,
        _mux: ala::Entity<MuxClient>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    pub(crate) fn focus(&self, _cx: &App) -> ala::FocusHandle {
        self.focus_handle.clone()
    }

    pub(crate) fn synchronize_descriptor(
        &mut self,
        _descriptor: &EditorDescriptor,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    pub(crate) fn set_window_corners(
        &mut self,
        _corners: crate::window::corners::WindowCorners,
        _cx: &mut Context<Self>,
    ) {
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().foreground.muted())
            .child("Editor panes are not included in this build")
    }
}
