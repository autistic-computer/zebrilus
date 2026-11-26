use super::*;

impl AppView {
    pub(super) fn ensure_pane_header(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        if self.terminal_headers.contains_key(&pane) {
            return;
        }
        let app = cx.entity().downgrade();
        let mux = self.mux.clone();
        let canvas = Rc::clone(&self.pane_canvas_bounds);
        let header = cx.new(|_| TerminalHeaderView {
            pane,
            app,
            mux,
            canvas,
            state: None,
        });
        self.terminal_headers.insert(pane, header);
    }

    pub(super) fn pane_content(
        &self,
        pane: PaneId,
        inactive: bool,
        inactive_opacity: f32,
        cx: &mut Context<Self>,
    ) -> Option<PaneContent> {
        if let Some((_, output)) = self
            .command_output
            .as_ref()
            .filter(|(output_pane, _)| *output_pane == pane)
        {
            output.update(cx, |output, cx| {
                output.set_text_dimmed(inactive, inactive_opacity, cx);
            });
            Some(PaneContent {
                view: AnyView::from(output.clone()),
                cached: true,
                inactive_style: PaneInactiveStyle::Content,
                covers_frame: false,
            })
        } else if let Some(picker) = self.pickers.get(&pane) {
            Some(PaneContent {
                view: AnyView::from(picker.clone()),
                cached: true,
                inactive_style: PaneInactiveStyle::Surface,
                covers_frame: false,
            })
        } else if let Some(agent) = self.agents.get(&pane) {
            Some(PaneContent {
                view: AnyView::from(agent.clone()),
                cached: true,
                inactive_style: PaneInactiveStyle::Surface,
                covers_frame: false,
            })
        } else if let Some(terminal) = self.terminals.get(&pane) {
            terminal.update(cx, |terminal, cx| {
                terminal.set_text_dimmed(inactive, inactive_opacity, cx);
            });
            Some(PaneContent {
                view: AnyView::from(terminal.clone()),
                cached: true,
                inactive_style: PaneInactiveStyle::Content,
                covers_frame: false,
            })
        } else if let Some(browser) = self.browsers.get(&pane) {
            browser.update(cx, |browser, cx| {
                browser.set_chrome_dimmed(inactive, inactive_opacity, cx);
            });
            Some(PaneContent {
                view: AnyView::from(browser.clone()),
                cached: false,
                inactive_style: PaneInactiveStyle::Content,
                covers_frame: true,
            })
        } else if let Some(editor) = self.editors.get(&pane) {
            Some(PaneContent {
                view: AnyView::from(editor.clone()),
                cached: false,
                inactive_style: PaneInactiveStyle::Surface,
                covers_frame: false,
            })
        } else if let Some(simulator) = self.simulators.get(&pane) {
            Some(PaneContent {
                view: AnyView::from(simulator.clone()),
                cached: false,
                inactive_style: PaneInactiveStyle::Surface,
                covers_frame: false,
            })
        } else {
            self.view_content(pane)
        }
    }

    pub(super) fn pane_kind_buttons(
        &self,
        pane: PaneId,
        cx: &App,
    ) -> Vec<zz_ui::pane::PaneKindButton> {
        if let Some(agent) = self.agents.get(&pane) {
            return agent.read(cx).header_buttons(cx);
        }
        #[cfg(feature = "agent-pane")]
        if let Some(view) = self.views.get(&pane) {
            return view.read(cx).header_buttons();
        }
        if let Some(simulator) = self.simulators.get(&pane) {
            return simulator.read(cx).header_buttons();
        }
        Vec::new()
    }

    pub(super) fn pane_title_menu(&self, pane: PaneId, cx: &App) -> Vec<zz_ui::pane::PaneTitleRow> {
        self.simulators
            .get(&pane)
            .map(|simulator| simulator.read(cx).title_menu())
            .unwrap_or_default()
    }

    pub(super) fn pane_title_pick(&mut self, pane: PaneId, index: usize, cx: &mut Context<Self>) {
        if let Some(simulator) = self.simulators.get(&pane) {
            simulator.update(cx, |simulator, cx| simulator.title_pick(index, cx));
            cx.notify();
        }
    }

    pub(super) fn pane_kind_action(
        &mut self,
        pane: PaneId,
        key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(agent) = self.agents.get(&pane) {
            agent.update(cx, |agent, cx| agent.header_action(key, window, cx));
            cx.notify();
            return;
        }
        #[cfg(feature = "agent-pane")]
        if let Some(view) = self.views.get(&pane) {
            view.update(cx, |view, cx| view.header_action(key, cx));
            cx.notify();
            return;
        }
        if let Some(simulator) = self.simulators.get(&pane) {
            simulator.update(cx, |simulator, cx| simulator.header_action(key, cx));
            cx.notify();
        }
    }

    pub(super) fn ensure_simulator(
        &mut self,
        pane: PaneId,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let descriptor = self.mux.read(cx).simulator(pane).unwrap_or_default();
        let floating = self
            .mux
            .read(cx)
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .any(|window| window.floating.iter().any(|float| float.pane == pane));
        let Some(simulator) = self.simulators.get(&pane) else {
            let mux = self.mux.clone();
            let simulator = cx.new(|cx| {
                let mut simulator =
                    crate::simulator::SimulatorView::new(pane, &descriptor, mux, cx);
                simulator.set_floating(floating, cx);
                simulator
            });
            self.simulators.insert(pane, simulator);
            self.simulator_devices.insert(pane, descriptor);
            return;
        };
        if self.simulator_devices.get(&pane) == Some(&descriptor) {
            return;
        }
        if simulator.update(cx, |simulator, cx| {
            simulator.synchronize_descriptor(&descriptor, cx)
        }) {
            self.pane_motion.relink(pane, window, cx);
        }
        self.simulator_devices.insert(pane, descriptor);
    }

    pub(super) fn pane_flip(&self, snapshot: &zz_protocol::PaneSnapshot) -> Option<PaneFlip> {
        PaneFlip::of(snapshot, self.flips.state(snapshot.id))
    }

    #[cfg(feature = "agent-pane")]
    fn view_content(&self, pane: PaneId) -> Option<PaneContent> {
        self.views.get(&pane).map(|view| PaneContent {
            view: AnyView::from(view.clone()),
            cached: false,
            inactive_style: PaneInactiveStyle::Surface,
            covers_frame: false,
        })
    }

    #[cfg(not(feature = "agent-pane"))]
    fn view_content(&self, _: PaneId) -> Option<PaneContent> {
        None
    }

    #[cfg(feature = "agent-pane")]
    pub(super) fn ensure_view(
        &mut self,
        pane: PaneId,
        descriptor: &zz_protocol::ViewDescriptor,
        vendor: Option<&'static zz_client::program::Program>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.views.get(&pane) {
            if view.update(cx, |view, cx| {
                view.synchronize_descriptor(descriptor, vendor, cx)
            }) {
                self.pane_motion.relink(pane, window, cx);
            }
            return;
        }
        let mux = self.mux.clone();
        let agents = self.agent_controller.clone();
        let view = cx.new(|cx| {
            crate::view_pane::ViewPaneView::new(pane, descriptor, vendor, mux, agents, cx)
        });
        self.views.insert(pane, view);
    }

    #[cfg(not(feature = "agent-pane"))]
    pub(super) fn ensure_view(
        &mut self,
        _: PaneId,
        _: &zz_protocol::ViewDescriptor,
        _: Option<&'static zz_client::program::Program>,
        _: &Window,
        _: &mut Context<Self>,
    ) {
    }

    pub(super) fn pane_action_keys(&self, cx: &App) -> zz_ui::pane::PaneActionKeys {
        let mux = self.mux.read(cx);
        crate::mux::prefix::pane_action_keys(
            mux.canonical_prefix().as_deref(),
            mux.prefix_bindings(),
        )
    }

    pub(super) fn pane_folder(&self, pane: &zz_protocol::PaneSnapshot, cx: &App) -> Option<String> {
        match self.agent_controller.read(cx).pane_cwd(pane.id) {
            Some(cwd) if matches!(pane.kind, PaneKindSnapshot::Agent(_)) => {
                Some(zz_client::providers::home_relative(&cwd.to_string_lossy()))
            }
            _ => zz_client::navigation::pane_folder(pane),
        }
    }
}

impl PaneContent {
    pub(super) fn element(&self) -> AnyElement {
        if self.cached {
            self.view
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element()
        } else {
            self.view.clone().into_any_element()
        }
    }
}
