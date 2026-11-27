use super::*;

impl BrowserView {
    pub(super) fn on_find_action(
        &mut self,
        _: &crate::menus::Find,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_find(window, cx);
        cx.stop_propagation();
    }

    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_open = true;
        let query = self.find_input.read(cx).value().to_string();
        let end = query.len();
        self.find_input.update(cx, |input, cx| {
            input.set_selected_range(0..end, cx);
        });
        self.find_input.read(cx).focus_handle(cx).focus(window, cx);
        if !query.is_empty() {
            let match_case = self.find_case_sensitive;
            self.controller.update(cx, |controller, cx| {
                controller.find(
                    self.pane,
                    self.active_tab,
                    &query,
                    true,
                    match_case,
                    false,
                    cx,
                );
            });
        }
        cx.notify();
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_open {
            return;
        }
        self.find_open = false;
        self.find_matches = None;
        self.controller.update(cx, |controller, cx| {
            controller.stop_finding(self.pane, self.active_tab, true, cx);
        });
        self.focus_page(window, cx);
        cx.notify();
    }

    pub(super) fn update_find(&mut self, cx: &mut Context<Self>) {
        if !self.find_open {
            return;
        }
        let query = self.find_input.read(cx).value().to_string();
        if query.is_empty() {
            self.find_matches = None;
            self.controller.update(cx, |controller, cx| {
                controller.stop_finding(self.pane, self.active_tab, true, cx);
            });
        } else {
            let match_case = self.find_case_sensitive;
            self.controller.update(cx, |controller, cx| {
                controller.find(
                    self.pane,
                    self.active_tab,
                    &query,
                    true,
                    match_case,
                    false,
                    cx,
                );
            });
        }
        cx.notify();
    }

    pub(super) fn step_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        let query = self.find_input.read(cx).value().to_string();
        if query.is_empty() {
            return;
        }
        let match_case = self.find_case_sensitive;
        self.controller.update(cx, |controller, cx| {
            controller.find(
                self.pane,
                self.active_tab,
                &query,
                forward,
                match_case,
                true,
                cx,
            );
        });
    }

    fn toggle_find_case(&mut self, cx: &mut Context<Self>) {
        self.find_case_sensitive = !self.find_case_sensitive;
        self.update_find(cx);
    }

    pub(super) fn render_find_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_query = !self.find_input.read(cx).value().is_empty();
        let state = BrowserFindState {
            case_sensitive: self.find_case_sensitive,
            has_query,
            matches: self.find_matches,
        };
        let view = cx.entity();
        browser_find_bar(
            &self.find_input,
            state,
            move |action, window, cx| {
                view.update(cx, |view, cx| match action {
                    BrowserFindAction::ToggleCase => view.toggle_find_case(cx),
                    BrowserFindAction::Previous => view.step_find(false, cx),
                    BrowserFindAction::Next => view.step_find(true, cx),
                    BrowserFindAction::Close => view.close_find(window, cx),
                });
            },
            cx,
        )
        .on_action(
            cx.listener(|view, _: &penna::components::input::Escape, window, cx| {
                view.close_find(window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }
}
