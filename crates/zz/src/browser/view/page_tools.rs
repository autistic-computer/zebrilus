use super::*;

impl BrowserView {
    pub(super) fn on_zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_zoom(ZoomOp::In, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_zoom(ZoomOp::Out, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_reset_zoom(&mut self, _: &ResetZoom, _: &mut Window, cx: &mut Context<Self>) {
        self.apply_zoom(ZoomOp::Reset, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_toggle_dev_tools(
        &mut self,
        _: &ToggleDevTools,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_dev_tools(cx);
        cx.stop_propagation();
    }

    pub(super) fn on_configured_element_selector(
        &mut self,
        action: &ConfiguredElementSelector,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if crate::keymap::action_for(cx, BROWSER_TABLE, &action.chord)
            != Some(ChromeAction::BrowserElementSelector)
        {
            cx.propagate();
            return;
        }
        self.on_element_pick(window, cx);
    }

    pub(super) fn toggle_dev_tools(&mut self, cx: &mut Context<Self>) {
        self.controller.update(cx, |controller, _| {
            controller.toggle_dev_tools(self.pane, self.active_tab);
        });
    }

    pub(super) fn apply_zoom(&mut self, op: ZoomOp, cx: &mut Context<Self>) {
        let result = self.controller.update(cx, |controller, _| match op {
            ZoomOp::In => controller.zoom_in(self.pane, self.active_tab),
            ZoomOp::Out => controller.zoom_out(self.pane, self.active_tab),
            ZoomOp::Reset => controller.reset_zoom(self.pane, self.active_tab),
        });
        if let Some((factor, percent)) = result {
            self.page_zoom_factor = factor;
            self.page_zoom_percent = percent;
            if self.element_pick_active && !self.start_element_pick(cx) {
                self.element_pick_active = false;
                self.emit_notification(
                    "Element picker is unavailable",
                    ClientMessageKind::Error,
                    cx,
                );
            }
            cx.notify();
        }
    }

    fn start_element_pick(&self, cx: &mut Context<Self>) -> bool {
        let appearance = element_picker_appearance(cx.theme(), self.page_zoom_factor);
        self.controller.update(cx, |controller, _| {
            controller.start_element_pick(self.pane, self.active_tab, &appearance)
        })
    }

    pub(super) fn on_element_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.element_pick_active {
            self.cancel_element_pick(cx);
        } else if self.start_element_pick(cx) {
            self.element_pick_active = true;
            self.focus_handle.focus(window, cx);
            self.controller.update(cx, |controller, _| {
                controller.set_focus(self.pane, true);
            });
        } else {
            self.emit_notification(
                "Element picker is unavailable",
                ClientMessageKind::Error,
                cx,
            );
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn cancel_element_pick(&mut self, cx: &mut Context<Self>) {
        if !self.element_pick_active {
            return;
        }
        self.controller.update(cx, |controller, _| {
            let _ = controller.cancel_element_pick(self.pane, self.active_tab);
        });
        self.element_pick_active = false;
    }
}
