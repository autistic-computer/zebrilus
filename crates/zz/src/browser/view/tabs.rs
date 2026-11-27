use super::*;

impl BrowserView {
    pub(super) fn active_tab_index(&self) -> usize {
        self.tabs
            .iter()
            .position(|tab| tab.id == self.active_tab)
            .unwrap_or(0)
    }

    fn save_active_tab_state(&mut self) {
        let index = self.active_tab_index();
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        tab.url.clone_from(&self.current_url);
        tab.title.clone_from(&self.title);
        tab.loading = self.loading;
        tab.loading_progress = self.loading_progress;
        tab.can_go_back = self.can_go_back;
        tab.can_go_forward = self.can_go_forward;
        tab.page_zoom_factor = self.page_zoom_factor;
        tab.page_zoom_percent = self.page_zoom_percent;
        tab.error.clone_from(&self.error);
    }

    pub(super) fn activate_tab(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if tab == self.active_tab {
            return;
        }
        let Some(index) = self.tabs.iter().position(|entry| entry.id == tab) else {
            return;
        };
        self.cancel_element_pick(cx);
        if self.find_open {
            self.find_open = false;
            self.find_matches = None;
            let previous = self.active_tab;
            self.controller.update(cx, |controller, cx| {
                controller.stop_finding(self.pane, previous, true, cx);
            });
        }
        self.status_message = None;
        self.context_menu = None;
        self.omnibox.reset();
        self.save_active_tab_state();
        self.active_tab = tab;
        self.ime_bounds = Arc::default();
        let entry = &self.tabs[index];
        self.current_url = entry.url.clone();
        self.title = entry.title.clone();
        self.loading = entry.loading;
        self.loading_progress = entry.loading_progress;
        self.can_go_back = entry.can_go_back;
        self.can_go_forward = entry.can_go_forward;
        self.page_zoom_factor = entry.page_zoom_factor;
        self.page_zoom_percent = entry.page_zoom_percent;
        self.error = entry.error.clone();
        self.reset_frame_state();
        let display = if is_blank_url(&self.current_url) {
            String::new()
        } else {
            self.current_url.clone()
        };
        let loading = self.loading;
        self.address.update(cx, |address, cx| {
            address.set_value(display, window, cx);
            address.set_loading(loading, window, cx);
        });
        self.controller.update(cx, |controller, cx| {
            controller.set_active_tab(self.pane, tab, cx);
        });
        if let Some(entry) = self.tabs.iter_mut().find(|entry| entry.id == tab)
            && !entry.started
        {
            entry.started = true;
            let request = browser_session_request(
                self.pane,
                self.current_url.clone(),
                self.profile.clone(),
                self.viewport,
                self.page_zoom_factor,
                self.gpu_context.clone(),
                &self.mux,
                cx,
            );
            self.controller.update(cx, |controller, cx| {
                controller.request_browser(self.pane, tab, request, cx);
            });
        }
        self.publish_tabs(cx);
        cx.notify();
    }

    pub(super) fn open_tab(
        &mut self,
        url: Option<&str>,
        foreground: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let url = url.map_or_else(
            || DEFAULT_URL.to_owned(),
            |url| normalize_url(url).unwrap_or_else(|_| DEFAULT_URL.to_owned()),
        );
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        self.tabs.push(BrowserTab::new(id, url.clone()));
        let request = browser_session_request(
            self.pane,
            url,
            self.profile.clone(),
            self.viewport,
            1.0,
            self.gpu_context.clone(),
            &self.mux,
            cx,
        );
        if foreground {
            self.activate_tab(id, window, cx);
        }
        self.controller.update(cx, |controller, cx| {
            controller.request_browser(self.pane, id, request, cx);
        });
        if foreground && self.shows_empty_state() {
            self.address.read(cx).focus_handle(cx).focus(window, cx);
        }
        self.publish_tabs(cx);
        cx.notify();
    }

    pub(super) fn adopt_popup(
        &mut self,
        opener: TabId,
        popup: SessionId,
        url: &str,
        foreground: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(opener_tab) = self.tabs.iter().find(|entry| entry.id == opener) else {
            return;
        };
        let (page_zoom_factor, page_zoom_percent) = if opener == self.active_tab {
            (self.page_zoom_factor, self.page_zoom_percent)
        } else {
            (opener_tab.page_zoom_factor, opener_tab.page_zoom_percent)
        };
        let id = TabId(self.next_tab_id);
        if !self.controller.update(cx, |controller, cx| {
            controller.adopt_popup(self.pane, opener, popup, id, cx)
        }) {
            return;
        }
        self.next_tab_id += 1;
        self.tabs.push(BrowserTab {
            page_zoom_factor,
            page_zoom_percent,
            popup: true,
            ..BrowserTab::new(id, url.to_owned())
        });
        if foreground {
            self.activate_tab(id, window, cx);
            self.focus_page(window, cx);
        }
        self.publish_tabs(cx);
        cx.notify();
    }

    pub(super) fn close_tab_by_id(
        &mut self,
        tab: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.len() <= 1 {
            return;
        }
        let Some(index) = self.tabs.iter().position(|entry| entry.id == tab) else {
            return;
        };
        if tab == self.active_tab {
            let neighbor = self
                .tabs
                .get(index + 1)
                .or_else(|| index.checked_sub(1).and_then(|index| self.tabs.get(index)))
                .map(|entry| entry.id);
            if let Some(neighbor) = neighbor {
                self.activate_tab(neighbor, window, cx);
            }
        }
        self.tabs.retain(|entry| entry.id != tab);
        self.pending_history_uses.remove(&tab);
        self.controller.update(cx, |controller, _| {
            controller.close_tab(self.pane, tab);
        });
        self.publish_tabs(cx);
        cx.notify();
    }

    fn cycle_tab(&mut self, offset: i64, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() <= 1 {
            return;
        }
        let len = i64::try_from(self.tabs.len()).unwrap_or(i64::MAX);
        let index = i64::try_from(self.active_tab_index()).unwrap_or_default();
        let next = usize::try_from((index + offset).rem_euclid(len)).unwrap_or_default();
        let tab = self.tabs[next].id;
        self.activate_tab(tab, window, cx);
    }

    pub(super) fn tab_strip_state(&self, cx: &App) -> (Vec<BrowserTabInfo>, usize) {
        let tabs = self
            .tabs
            .iter()
            .map(|tab| {
                let (url, title) = if tab.id == self.active_tab {
                    (self.current_url.as_str(), self.title.as_str())
                } else {
                    (tab.url.as_str(), tab.title.as_str())
                };
                let detail = if title.is_empty() { url } else { title };
                let label = if title.is_empty() {
                    tab_host_label(url)
                } else {
                    title.to_owned().into()
                };
                BrowserTabInfo::new(tab.id.0, label, detail.to_owned())
                    .favicon(recent_pages::favicon(&self.profile, url, cx))
            })
            .collect();
        (tabs, self.active_tab_index())
    }

    pub(super) fn on_new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.open_tab(None, true, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_close_pane(
        &mut self,
        _: &ClosePane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.len() > 1 {
            self.close_tab_by_id(self.active_tab, window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn on_next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_tab(1, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_previous_tab(
        &mut self,
        _: &PreviousTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_tab(-1, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_select_tab(
        &mut self,
        action: &SelectTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.tabs.get(action.index).map(|tab| tab.id) {
            self.activate_tab(tab, window, cx);
        }
        cx.stop_propagation();
    }

    pub(super) fn on_select_last_tab(
        &mut self,
        _: &SelectLastTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.tabs.last().map(|tab| tab.id) {
            self.activate_tab(tab, window, cx);
        }
        cx.stop_propagation();
    }

    pub(crate) fn synchronize_tabs(
        &mut self,
        tabs: &[String],
        active_tab: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if tabs.is_empty() || (self.mux_tabs == tabs && self.mux_active == active_tab) {
            return;
        }
        let local = self.tab_urls();
        let order = zz_client::navigation::tab_order(
            &local.iter().map(String::as_str).collect::<Vec<_>>(),
            tabs,
        );
        self.mux_tabs = tabs.to_vec();
        self.mux_active = active_tab;
        self.applying_mux = true;
        if let Some(order) = order {
            let mut slots = std::mem::take(&mut self.tabs)
                .into_iter()
                .map(Some)
                .collect::<Vec<_>>();
            self.tabs = order
                .into_iter()
                .filter_map(|index| slots[index].take())
                .collect();
        }
        while self.tabs.len() > tabs.len() {
            let id = self.tabs[self.tabs.len() - 1].id;
            self.close_tab_by_id(id, window, cx);
        }
        for (index, url) in tabs.iter().enumerate() {
            if index < self.tabs.len() {
                self.navigate_tab_at(index, url, cx);
            } else {
                let id = TabId(self.next_tab_id);
                self.next_tab_id += 1;
                let url = normalize_url(url).unwrap_or_else(|_| DEFAULT_URL.to_owned());
                self.tabs.push(BrowserTab::restored(id, url));
            }
        }
        let active = self.tabs[active_tab.min(self.tabs.len() - 1)].id;
        self.activate_tab(active, window, cx);
        self.applying_mux = false;
        cx.notify();
    }

    fn navigate_tab_at(&mut self, index: usize, url: &str, cx: &mut Context<Self>) {
        let url = match normalize_url(url) {
            Ok(url) => url,
            Err(error) => {
                log::warn!("ignoring mux url for browser pane {}: {error}", self.pane);
                return;
            }
        };
        let Some(entry) = self.tabs.get_mut(index) else {
            return;
        };
        let tab = entry.id;
        self.pending_history_uses.remove(&tab);
        if tab == self.active_tab {
            if url == self.current_url {
                return;
            }
            self.cancel_element_pick(cx);
            self.error = None;
            self.current_url.clone_from(&url);
        } else {
            if entry.url == url {
                return;
            }
            entry.url.clone_from(&url);
            if !entry.started {
                return;
            }
        }
        self.controller.update(cx, |controller, cx| {
            controller.navigate(self.pane, tab, &url, cx);
        });
    }

    pub(super) fn tab_urls(&self) -> Vec<String> {
        self.tabs
            .iter()
            .map(|tab| {
                if tab.id == self.active_tab {
                    self.current_url.clone()
                } else {
                    tab.url.clone()
                }
            })
            .collect()
    }

    pub(super) fn move_tab(&mut self, tab: TabId, to: usize, cx: &mut Context<Self>) {
        let Some(from) = self.tabs.iter().position(|entry| entry.id == tab) else {
            return;
        };
        if to >= self.tabs.len() || to == from {
            return;
        }
        let entry = self.tabs.remove(from);
        self.tabs.insert(to, entry);
        self.publish_tabs(cx);
        cx.notify();
    }
}
