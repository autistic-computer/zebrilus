use super::*;

impl AppView {
    pub(super) fn ensure_picker(
        &mut self,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pickers.contains_key(&pane) {
            return;
        }
        let mux = self.mux.clone();
        let view = cx.new(|cx| PanePickerView::new(pane, mux, cx));
        cx.subscribe_in(&view, window, |this, _, chosen: &AgentChosen, _, cx| {
            this.on_agent_chosen(chosen, cx);
        })
        .detach();
        cx.subscribe_in(
            &view,
            window,
            |this, picker, choose: &ChooseFolder, window, cx| {
                this.open_folder_palette(picker, choose, window, cx);
            },
        )
        .detach();
        self.pickers.insert(pane, view);
    }

    pub(super) fn on_choose_machine(&mut self, choose: &ChooseMachine, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.new_chat(choose.host, cx));
    }

    fn on_agent_chosen(&mut self, chosen: &AgentChosen, cx: &mut Context<Self>) {
        let host = self.mux.read(cx).attached_host();
        self.sidebar.update(cx, |sidebar, _| {
            sidebar.follow_moved_window(host, chosen.window, chosen.from);
        });
    }

    pub(super) fn ensure_agent_view(
        &mut self,
        pane: PaneId,
        agent: &zz_protocol::AgentDescriptor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "agent-pane")]
        {
            if !self.agents.contains_key(&pane)
                && let Some(view) = self.parked_agents.remove(&(self.agents_host, pane))
            {
                let host = self.agents_host;
                self.agent_controller
                    .update(cx, |controller, _| controller.unpark_pane(host, pane));
                self.agents.insert(pane, view);
            }
            let from_seq = self.agent_controller.read(cx).last_applied(pane);
            self.mux
                .update(cx, |mux, _| mux.watch_agent(pane, from_seq));
        }
        if self.agents.contains_key(&pane) {
            return;
        }
        let controller = self.agent_controller.clone();
        let mux = self.mux.clone();
        let provider = agent.provider;
        let pane_canvas = Rc::clone(&self.pane_canvas_bounds);
        let view = cx.new(|cx| {
            let mut view = AgentView::new(pane, provider, controller, mux, window, cx);
            view.share_pane_canvas(pane_canvas);
            view
        });
        #[cfg(feature = "agent-pane")]
        cx.subscribe_in(
            &view,
            window,
            move |this, _, picked: &crate::agent::AgentAccountPicked, window, cx| {
                this.switch_agent_account(pane, picked.account.as_deref(), picked.mode, window, cx);
            },
        )
        .detach();
        self.agents.insert(pane, view);
    }

    #[cfg(feature = "agent-pane")]
    pub(super) fn follow_agent_host(&mut self, cx: &mut Context<Self>) {
        let host = self.mux.read(cx).attached_host();
        if host == self.agents_host {
            return;
        }
        let from = std::mem::replace(&mut self.agents_host, host);
        let parkable = parkable_agents(self.mux.read(cx));
        for (pane, view) in std::mem::take(&mut self.agents) {
            self.park_agent(from, pane, view, &parkable, cx);
        }
        self.agent_controller.update(cx, |controller, cx| {
            controller.retain_panes(&BTreeSet::new(), &BTreeSet::new(), cx);
        });
    }

    #[cfg(feature = "agent-pane")]
    pub(super) fn park_agents(
        &mut self,
        wanted: &BTreeSet<PaneId>,
        existing: &BTreeSet<PaneId>,
        cx: &mut Context<Self>,
    ) {
        let leaving = self
            .agents
            .keys()
            .filter(|pane| !wanted.contains(pane))
            .copied()
            .collect::<Vec<_>>();
        if leaving.is_empty() && self.parked_agents.is_empty() {
            return;
        }
        let parkable = parkable_agents(self.mux.read(cx));
        let host = self.agents_host;
        for pane in leaving {
            let Some(view) = self.agents.remove(&pane) else {
                continue;
            };
            if existing.contains(&pane) && !parkable.contains(&(host, pane)) {
                let (text, images) = view.read(cx).draft(cx);
                self.agent_controller.update(cx, |controller, _| {
                    controller.keep_draft(pane, &text, images);
                });
            }
            self.park_agent(host, pane, view, &parkable, cx);
        }
        self.parked_agents.retain(|key, _| parkable.contains(key));
        self.agent_controller
            .update(cx, |controller, _| controller.retain_parked(&parkable));
    }

    #[cfg(feature = "agent-pane")]
    fn park_agent(
        &mut self,
        host: HostId,
        pane: PaneId,
        view: Entity<AgentView>,
        parkable: &BTreeSet<(HostId, PaneId)>,
        cx: &mut Context<Self>,
    ) {
        self.mux.update(cx, |mux, _| mux.unwatch_agent(pane));
        if !parkable.contains(&(host, pane)) {
            return;
        }
        view.update(cx, |view, cx| view.set_visible(false, cx));
        self.agent_controller
            .update(cx, |controller, _| controller.park_pane(host, pane));
        self.parked_agents.insert((host, pane), view);
    }

    #[cfg(feature = "agent-pane")]
    fn switch_agent_account(
        &mut self,
        pane: PaneId,
        account: Option<&str>,
        mode: zz_client::agent_flip::FlipMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mux = self.mux.clone();
        if self
            .flips
            .switch_account_when(pane, account, mode, |command| {
                mux.read(cx).execute_answered(command)
            })
        {
            self.refresh_flip(pane, window, cx);
        }
    }

    pub(super) fn flip_pane(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.mux.read(cx).snapshot();
        let Some(target) = snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane))
            .and_then(zz_client::agent_flip::flip_target)
        else {
            return;
        };
        let mux = self.mux.clone();
        if self.flips.press(pane, target.to, |command| {
            mux.read(cx).execute_answered(command)
        }) {
            self.refresh_flip(pane, window, cx);
        }
    }

    fn choose_flip(
        &mut self,
        pane: PaneId,
        choice: zz_ui::pane::FlipChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mode) = choice.mode() else {
            self.flips.dismiss(pane);
            self.refresh_flip(pane, window, cx);
            return;
        };
        let mux = self.mux.clone();
        if self
            .flips
            .choose(pane, mode, |command| mux.read(cx).execute_answered(command))
        {
            self.refresh_flip(pane, window, cx);
        }
    }

    fn refresh_flip(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        self.synchronized_signature = None;
        self.synchronize_panes(window, cx);
        if let Some((_, focus)) = self.pane_focus_target(pane, cx) {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn drain_flip_answers(&mut self, cx: &mut Context<Self>) {
        if !self.mux.read(cx).has_command_answers() {
            return;
        }
        let answers = self.mux.update(cx, |mux, _| mux.take_command_answers());
        for (request, result) in answers {
            if !self.flips.owns(request) {
                continue;
            }
            if let Some(error) = self
                .flips
                .settle(request, result.as_ref().copied().map_err(String::as_str))
            {
                self.mux.update(cx, |_, cx| {
                    MuxClient::emit_notification(ClientMessageKind::Error, error, cx);
                });
            }
        }
        cx.notify();
    }

    pub(super) fn flip_overlay(&self, pane: PaneId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let flip = self.flips.state(pane);
        if *flip == zz_client::agent_flip::Flip::Idle {
            return None;
        }
        let to = self.flips_target(pane, cx)?;
        let app = cx.entity().downgrade();
        zz_ui::pane::pane_flip_overlay(
            pane,
            to,
            flip,
            move |choice, window, cx| {
                if let Some(app) = app.upgrade() {
                    app.update(cx, |view, cx| view.choose_flip(pane, choice, window, cx));
                }
            },
            cx,
        )
    }

    fn flips_target(&self, pane: PaneId, cx: &App) -> Option<zz_ui::pane::FlipTo> {
        self.mux
            .read(cx)
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane))
            .and_then(zz_client::agent_flip::flip_target)
            .map(|target| target.to)
    }

    #[cfg(feature = "agent-pane")]
    pub(super) fn drain_agent_events(&mut self, cx: &mut Context<Self>) {
        if !self.mux.read(cx).has_agent_events() {
            return;
        }
        let registered = self.agent_controller.read(cx).registered_panes();
        let before = self.agent_controller.read(cx).moves();
        let events = self
            .mux
            .update(cx, |mux, _| mux.take_agent_events_for(&registered));
        let moved = self.agent_controller.update(cx, |controller, cx| {
            for (pane, items) in events.items {
                controller.apply_stream_items(pane, items, cx);
            }
            for (pane, state) in events.states {
                controller.apply_pane_state(pane, &state, cx);
            }
            for (pane, reply) in events.sessions {
                controller.apply_sessions(pane, &reply, cx);
            }
            controller.moves() != before
        });
        if moved {
            cx.notify();
        }
    }

    #[cfg(not(feature = "agent-pane"))]
    pub(super) fn drain_agent_events(&mut self, _cx: &mut Context<Self>) {}
}
