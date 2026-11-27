use super::*;

impl BrowserView {
    pub(super) fn on_back(&mut self, cx: &mut Context<Self>) {
        if self.can_go_back {
            self.cancel_element_pick(cx);
            self.pending_history_uses.remove(&self.active_tab);
            self.controller.update(cx, |controller, cx| {
                controller.go_back(self.pane, self.active_tab, cx);
            });
        }
        cx.stop_propagation();
    }

    pub(super) fn on_forward(&mut self, cx: &mut Context<Self>) {
        if self.can_go_forward {
            self.cancel_element_pick(cx);
            self.pending_history_uses.remove(&self.active_tab);
            self.controller.update(cx, |controller, cx| {
                controller.go_forward(self.pane, self.active_tab, cx);
            });
        }
        cx.stop_propagation();
    }

    pub(super) fn on_reload(&mut self, cx: &mut Context<Self>) {
        self.cancel_element_pick(cx);
        self.pending_history_uses.remove(&self.active_tab);
        self.error = None;
        self.controller.update(cx, |controller, cx| {
            controller.reload(self.pane, self.active_tab, cx);
        });
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn on_hard_reload(&mut self, cx: &mut Context<Self>) {
        self.cancel_element_pick(cx);
        self.pending_history_uses.remove(&self.active_tab);
        self.error = None;
        self.controller.update(cx, |controller, cx| {
            controller.reload_ignore_cache(self.pane, self.active_tab, cx);
        });
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn on_retry(&mut self, cx: &mut Context<Self>) {
        if self.recoverable {
            self.recreate_session(cx);
        }
        cx.stop_propagation();
    }

    pub(super) fn on_go_back(&mut self, _: &GoBack, _: &mut Window, cx: &mut Context<Self>) {
        self.on_back(cx);
    }

    pub(super) fn on_go_forward(&mut self, _: &GoForward, _: &mut Window, cx: &mut Context<Self>) {
        self.on_forward(cx);
    }

    pub(super) fn on_reload_action(&mut self, _: &Reload, _: &mut Window, cx: &mut Context<Self>) {
        self.on_reload(cx);
    }

    pub(super) fn on_hard_reload_action(
        &mut self,
        _: &HardReload,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.on_hard_reload(cx);
    }
}
