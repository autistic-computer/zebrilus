use super::*;

impl BrowserView {
    pub(super) fn update_omnibox(&mut self, value: &str, cx: &mut Context<Self>) -> Option<String> {
        if !self.address_editing {
            let previous_value = if is_blank_url(&self.current_url) {
                ""
            } else {
                &self.current_url
            };
            self.omnibox.begin(previous_value);
            self.address_editing = true;
        }
        let suggestions =
            recent_pages::suggestions(&self.profile, value, OMNIBOX_SUGGESTION_LIMIT, cx);
        let completion = self.omnibox.update(value, suggestions);
        cx.notify();
        completion
    }

    pub(super) fn accept_omnibox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (value, history_use) = self.omnibox.accept(&self.address.read(cx).value());
        let history_use = history_use.map(|history_use| PendingHistoryUse {
            profile: self.profile.clone(),
            input: history_use.input,
            selected: history_use.selected,
            started: false,
        });
        self.navigate_address(&value, history_use, cx);
        self.focus_page(window, cx);
    }

    pub(super) fn submit_address(&mut self, value: &str, cx: &mut Context<Self>) {
        self.navigate_address(value, None, cx);
    }

    fn navigate_address(
        &mut self,
        value: &str,
        history_use: Option<PendingHistoryUse>,
        cx: &mut Context<Self>,
    ) {
        if value.trim().is_empty() {
            return;
        }
        self.pending_history_uses.remove(&self.active_tab);
        match resolve_address(value, crate::config::browser_search_provider(cx)) {
            Ok(url) => {
                self.cancel_element_pick(cx);
                self.error = None;
                self.current_url.clone_from(&url);
                if let Some(history_use) = history_use {
                    self.pending_history_uses
                        .insert(self.active_tab, history_use);
                }
                self.controller.update(cx, |controller, cx| {
                    controller.navigate(self.pane, self.active_tab, &url, cx);
                });
            }
            Err(error) => self.error = Some(Arc::from(error.to_string())),
        }
        cx.notify();
    }

    pub(super) fn complete_history_use(
        &mut self,
        tab: TabId,
        url: &str,
        succeeded: bool,
        cx: &mut Context<Self>,
    ) {
        if !self
            .pending_history_uses
            .get(&tab)
            .is_some_and(|history_use| history_use.started)
        {
            return;
        }
        let Some(history_use) = self.pending_history_uses.remove(&tab) else {
            return;
        };
        if succeeded {
            recent_pages::record_omnibox_use(
                &history_use.profile,
                &history_use.input,
                url,
                history_use.selected,
                cx,
            );
        }
    }

    pub(super) fn mark_history_use_started(&mut self, tab: TabId) {
        if let Some(history_use) = self.pending_history_uses.get_mut(&tab) {
            history_use.started = true;
        }
    }

    pub(super) fn on_focus_address(
        &mut self,
        _: &FocusAddress,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.address.read(cx).focus_handle(cx).focus(window, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_omnibox_next(
        &mut self,
        _: &OmniboxNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_omnibox_suggestion(1, window, cx);
    }

    pub(super) fn on_omnibox_previous(
        &mut self,
        _: &OmniboxPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_omnibox_suggestion(-1, window, cx);
    }

    fn select_omnibox_suggestion(
        &mut self,
        direction: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(preview) = self
            .address_editing
            .then(|| self.omnibox.step(direction))
            .flatten()
        else {
            cx.propagate();
            return;
        };
        self.address.update(cx, |address, cx| {
            if address.value().as_ref() != preview {
                address.set_value(preview.clone(), window, cx);
            }
            address.set_selected_range(preview.len()..preview.len(), cx);
        });
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn on_omnibox_delete(
        &mut self,
        _: &OmniboxDelete,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.omnibox.selected else {
            cx.propagate();
            return;
        };
        let Some(url) = self
            .omnibox
            .suggestions
            .get(index)
            .map(|suggestion| suggestion.url.clone())
        else {
            cx.propagate();
            return;
        };
        recent_pages::remove(&self.profile, &url, cx);
        let query = self.omnibox.query.clone();
        self.omnibox.suggestions =
            recent_pages::suggestions(&self.profile, &query, OMNIBOX_SUGGESTION_LIMIT, cx);
        self.omnibox.selected = None;
        self.address.update(cx, |address, cx| {
            address.set_value(query.clone(), window, cx);
            address.set_selected_range(query.len()..query.len(), cx);
        });
        cx.stop_propagation();
        cx.notify();
    }

    fn accept_omnibox_suggestion(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.omnibox.suggestions.len() {
            return;
        }
        self.omnibox.selected = Some(index);
        self.accept_omnibox(window, cx);
    }

    pub(super) fn render_omnibox_results(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.address_editing || self.omnibox.suggestions.is_empty() {
            return None;
        }
        let view = cx.entity();
        let rows = self
            .omnibox
            .suggestions
            .iter()
            .enumerate()
            .map(|(index, suggestion)| {
                let view = view.clone();
                browser_omnibox_row(
                    index,
                    suggestion.title.clone(),
                    suggestion.display_url.clone(),
                    self.omnibox.selected == Some(index),
                    recent_pages::favicon(&self.profile, &suggestion.url, cx),
                    px(32.0),
                    cx,
                )
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.accept_omnibox_suggestion(index, window, cx);
                    });
                    cx.stop_propagation();
                })
                .into_any_element()
            })
            .collect();
        Some(browser_omnibox_panel(rows, cx).into_any_element())
    }

    pub(super) fn render_empty_state(
        &self,
        radii: Corners<Pixels>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let recents = dedupe_recents(
            recent_pages::recent(&self.profile, cx, EMPTY_STATE_RECENT_LIMIT * 8),
            EMPTY_STATE_RECENT_LIMIT,
        );
        let address_focus_handle = self.address.read(cx).focus_handle(cx);
        let empty_state_view = cx.entity();
        let content: AnyElement = if recents.is_empty() {
            BrowserEmptyHint.into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .w_full()
                .max_w(px(360.0))
                .gap(px(4.0))
                .children(
                    recents
                        .iter()
                        .enumerate()
                        .map(|(index, page)| {
                            Self::render_recent_row(index, page, cx).into_any_element()
                        })
                        .collect::<Vec<_>>(),
                )
                .into_any_element()
        };
        round_div_radii(
            browser_start_surface(content).on_mouse_down(
                MouseButton::Left,
                move |_, window, cx| {
                    empty_state_view.update(cx, |view, cx| view.select_pane(cx));
                    address_focus_handle.focus(window, cx);
                },
            ),
            radii,
        )
    }
}
