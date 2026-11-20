use ala::{
    Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div,
};
use penna::{ActiveTheme as _, Colorize as _, Label, TextStyle};
use zz_protocol::{PaneId, SimulatorDescriptor};
use zz_ui::pane::PaneKindButton;

use crate::mux::client::MuxClient;

pub(crate) struct SimulatorView {
    focus: FocusHandle,
}

impl SimulatorView {
    pub(crate) fn new(
        _: PaneId,
        _: &SimulatorDescriptor,
        _: Entity<MuxClient>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }

    pub(crate) fn synchronize_descriptor(
        &mut self,
        _: &SimulatorDescriptor,
        _: &mut Context<Self>,
    ) -> bool {
        false
    }

    pub(crate) fn focus(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub(crate) fn title(&self) -> SharedString {
        "Simulator".into()
    }

    pub(crate) fn header_buttons(&self) -> Vec<PaneKindButton> {
        Vec::new()
    }

    pub(crate) fn header_action(&mut self, _: &str, _: &mut Context<Self>) {}
}

impl Render for SimulatorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.focus)
            .child(
                Label::new(TextStyle::Caption, "Simulators need a Mac")
                    .text_color(cx.theme().foreground.muted()),
            )
    }
}
