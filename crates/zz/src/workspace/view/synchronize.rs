use super::*;

impl AppView {
    pub(super) fn synchronize_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let revision = AppRevision::for_mux(self.mux.read(cx));
        let (sidebar_focused, route, chat_mode, chat_landing, chat_target) = {
            let sidebar = self.sidebar.read(cx);
            (
                sidebar.is_focused(window),
                sidebar.route(),
                sidebar.chat_mode(),
                sidebar.chat_landing(),
                sidebar.chat_target(),
            )
        };
        if let Some(last) = &self.synchronized_signature
            && last.revision == revision
            && last.sidebar_focused == sidebar_focused
            && last.route == route
            && last.chat_mode == chat_mode
            && last.chat_landing == chat_landing
            && last.chat_target == chat_target
        {
            return;
        }
        let previous_route = self.synchronized_signature.as_ref().map(|last| last.route);
        let entering_settings =
            route == WorkspaceRoute::Settings && previous_route != Some(WorkspaceRoute::Settings);
        if route == WorkspaceRoute::App && previous_route == Some(WorkspaceRoute::Settings) {
            self.pane_focus_owed = true;
        }
        if self
            .synchronized_signature
            .as_ref()
            .is_some_and(|last| last.revision.attached_host != revision.attached_host)
        {
            self.pane_focus_owed = true;
        }
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let mux = self.mux.read(cx);
        let attached = mux.attached_session();
        let snapshot = mux.snapshot();
        let command_output = mux.command_output();
        let menu = mux.menu().cloned();
        let confirm = mux.confirm().cloned();
        let command_prompt = mux.command_prompt().cloned();
        let command_prompt_revision = mux.command_prompt_revision();
        let choose_tree = mux.choose_tree().cloned();
        let choose_tree_revision = mux.choose_tree_revision();
        if self
            .pending_palette_chooser_close
            .is_some_and(|closed_revision| closed_revision != mux.choose_tree_closed_revision())
        {
            self.pending_palette_chooser_close = None;
        }
        let choose_buffer = mux.choose_buffer().cloned();
        let choose_buffer_revision = mux.choose_buffer_revision();
        let display_panes = mux.display_panes().cloned();
        let display_panes_revision = mux.display_panes_revision();
        let sidebar_focus_revision = mux.sidebar_focus_revision();
        let sidebar_focus_requested = sidebar_focus_revision != self.sidebar_focus_revision;
        let bell_revision = mux.bell_revision();
        let empty_workspace_visible = chat_landing
            || !mux.has_hosts()
            || empty_workspace_available(
                snapshot.generation,
                snapshot.sessions.len(),
                mux.error().is_some(),
            );
        self.empty_workspace_focus_pending = empty_workspace_focus_owed(
            empty_workspace_visible,
            self.empty_workspace_visible,
            self.empty_workspace_focus_pending,
        );
        if self.empty_workspace_visible && !empty_workspace_visible {
            self.pane_focus_owed = true;
        }
        self.empty_workspace_visible = empty_workspace_visible;
        if bell_revision != self.bell_revision {
            self.bell_revision = bell_revision;
            window.request_attention();
            crate::agent::sound::play(zz_client::attention::Chime::Bell, cx);
        }
        let mut wanted_pickers = BTreeSet::new();
        let mut wanted_terminals = BTreeSet::new();
        let mut wanted_browsers = BTreeSet::new();
        let mut wanted_agents = BTreeSet::new();
        let mut wanted_editors = BTreeSet::new();
        let mut wanted_views = BTreeSet::new();
        let mut wanted_simulators = BTreeSet::new();
        let mut existing = BTreeSet::new();
        self.flips.observe(&snapshot);
        let active_window = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == attached)
            .map(|session| snapshot.focused_window_for(session));

        #[cfg(feature = "agent-pane")]
        self.follow_agent_host(cx);
        let whole = &snapshot;
        for session in snapshot
            .sessions
            .iter()
            .filter(|session| Some(session.id) == attached)
        {
            for mux_window in &session.windows {
                for (pane, snapshot) in &mux_window.panes {
                    existing.insert(*pane);
                    match &snapshot.kind {
                        PaneKindSnapshot::Picker => {
                            wanted_pickers.insert(*pane);
                            self.ensure_picker(*pane, window, cx);
                        }
                        PaneKindSnapshot::Terminal => {
                            if agent_picker_place(whole, *pane, chat_mode).is_some() {
                                wanted_pickers.insert(*pane);
                                self.ensure_picker(*pane, window, cx);
                            }
                            wanted_terminals.insert(*pane);
                            if !self.terminals.contains_key(pane) {
                                let mux = self.mux.clone();
                                let terminal_resize_suppressed =
                                    Rc::clone(&self.terminal_resize_suppressed);
                                let pane = *pane;
                                let view = cx.new(|cx| {
                                    TerminalView::new(
                                        pane,
                                        mux,
                                        terminal_resize_suppressed,
                                        window,
                                        cx,
                                    )
                                });
                                self.terminals.insert(pane, view);
                            }
                            self.ensure_pane_header(*pane, cx);
                        }
                        PaneKindSnapshot::Browser(browser) => {
                            wanted_browsers.insert(*pane);
                            if !self.browsers.contains_key(pane) {
                                let controller = self.controller.clone();
                                let mux = self.mux.clone();
                                let pane = *pane;
                                let descriptor = browser.clone();
                                let resize_held = Rc::clone(&self.terminal_resize_suppressed);
                                let pane_canvas = Rc::clone(&self.pane_canvas_bounds);
                                let view = cx.new(|cx| {
                                    let mut view = BrowserView::new(
                                        pane,
                                        &descriptor,
                                        controller,
                                        mux,
                                        window,
                                        cx,
                                    );
                                    view.share_resize_hold(resize_held);
                                    view.share_pane_canvas(pane_canvas);
                                    view
                                });
                                cx.subscribe(&view, |view, _, drag: &PaneDrag, cx| {
                                    view.on_pane_drag_start(*drag, cx);
                                })
                                .detach();
                                self.browsers.insert(pane, view);
                            } else if let Some(view) = self.browsers.get(pane) {
                                view.update(cx, |view, cx| {
                                    view.synchronize_profile(&browser.profile, cx);
                                    view.synchronize_tabs(
                                        &browser.tabs,
                                        browser.active_tab,
                                        window,
                                        cx,
                                    );
                                });
                            }
                        }
                        PaneKindSnapshot::Agent(agent) => {
                            wanted_agents.insert(*pane);
                            self.ensure_agent_view(*pane, agent, window, cx);
                            self.ensure_pane_header(*pane, cx);
                        }
                        PaneKindSnapshot::Editor(editor) => {
                            wanted_editors.insert(*pane);
                            if let Some(view) = self.editors.get(pane) {
                                view.update(cx, |view, cx| {
                                    view.synchronize_descriptor(
                                        editor,
                                        &snapshot.title,
                                        window,
                                        cx,
                                    );
                                });
                            } else {
                                let mux = self.mux.clone();
                                let pane = *pane;
                                let descriptor = editor.clone();
                                let title = snapshot.title.clone();
                                let view = cx.new(|cx| {
                                    EditorView::new(pane, &descriptor, &title, mux, window, cx)
                                });
                                self.editors.insert(pane, view);
                            }
                        }
                        PaneKindSnapshot::View(descriptor) => {
                            wanted_views.insert(*pane);
                            let vendor = descriptor
                                .origin
                                .and_then(|origin| mux_window.panes.get(&origin))
                                .and_then(|origin| match &origin.kind {
                                    PaneKindSnapshot::Agent(agent) => {
                                        zz_client::window_summary::provider_program(agent.provider)
                                    }
                                    _ => None,
                                });
                            self.ensure_view(*pane, descriptor, vendor, window, cx);
                            self.ensure_pane_header(*pane, cx);
                        }
                        PaneKindSnapshot::Other { kind } => {
                            if kind == zz_protocol::SIMULATOR_PANE_KIND {
                                wanted_simulators.insert(*pane);
                                self.ensure_simulator(*pane, window, cx);
                                self.ensure_pane_header(*pane, cx);
                            }
                        }
                    }
                }
            }
        }

        match command_output {
            Some(output) => {
                let current_matches = self
                    .command_output
                    .as_ref()
                    .is_some_and(|(pane, _)| *pane == output.pane);
                if !current_matches {
                    let mux = self.mux.clone();
                    let pane = output.pane;
                    let view = cx.new(|cx| TerminalView::new_command_output(pane, mux, window, cx));
                    self.command_output = Some((pane, view));
                }
            }
            None => {
                if self.command_output.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        match menu {
            Some(state) => {
                if let Some(menu) = &self.menu {
                    menu.update(cx, |menu, cx| menu.synchronize(state, cx));
                } else {
                    let mux = self.mux.clone();
                    self.menu = Some(cx.new(|cx| MenuView::new(mux, state, cx)));
                }
            }
            None => {
                if self.menu.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        match confirm {
            Some(state) => {
                if let Some(confirm) = &self.confirm {
                    confirm.update(cx, |confirm, cx| confirm.synchronize(state, cx));
                } else {
                    let mux = self.mux.clone();
                    self.confirm = Some(cx.new(|cx| ConfirmView::new(mux, state, cx)));
                }
            }
            None => {
                if self.confirm.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        if let Some((pane, output)) = &self.command_output {
            let commands = self
                .mux
                .update(cx, |mux, _| mux.take_terminal_commands(*pane));
            for command in commands {
                output.update(cx, |output, cx| {
                    output.apply_ui_command(command, window, cx);
                });
            }
        }

        match command_prompt.as_ref() {
            Some(_)
                if self.command_palette.as_ref().is_some_and(|palette| {
                    palette.read(cx).is_local()
                        && self.local_palette_prompt_revision == Some(command_prompt_revision)
                }) =>
            {
                if let Some(palette) = &self.command_palette {
                    palette.update(cx, |palette, cx| palette.refresh(window, cx));
                }
            }
            Some(state) => {
                if self.command_palette.as_ref().is_none_or(|palette| {
                    palette.read(cx).is_local() || palette.read(cx).is_window_chooser()
                }) {
                    self.local_palette_prompt_revision = None;
                    let mux = self.mux.clone();
                    let snapshot = Arc::clone(&snapshot);
                    self.command_palette = Some(cx.new(|cx| {
                        CommandPaletteView::new(
                            palette_backend(mux),
                            state,
                            command_prompt_revision,
                            snapshot,
                            window,
                            cx,
                        )
                    }));
                }
                if let Some(palette) = &self.command_palette {
                    palette.update(cx, |palette, cx| {
                        palette.synchronize(state, command_prompt_revision, &snapshot, window, cx);
                    });
                }
            }
            None if choose_tree
                .as_ref()
                .is_some_and(|state| state.kind == zz_protocol::ChooseTreeKind::Windows)
                && self.pending_palette_chooser_close.is_none()
                && !self.command_palette.as_ref().is_some_and(|palette| {
                    palette.read(cx).is_local()
                        && self.local_palette_chooser_revision == Some(choose_tree_revision)
                }) =>
            {
                let state = choose_tree.as_ref().expect("window chooser state");
                if self
                    .command_palette
                    .as_ref()
                    .is_none_or(|palette| !palette.read(cx).is_window_chooser())
                {
                    self.local_palette_prompt_revision = None;
                    self.local_palette_chooser_revision = None;
                    let mux = self.mux.clone();
                    let palette = cx.new(|cx| {
                        CommandPaletteView::new_window_chooser(
                            palette_backend(mux),
                            state,
                            choose_tree_revision,
                            window,
                            cx,
                        )
                    });
                    self.observe_command_palette(&palette, window, cx);
                    self.command_palette = Some(palette);
                }
                if let Some(palette) = &self.command_palette {
                    palette.update(cx, |palette, cx| {
                        palette.synchronize_window_chooser(state, choose_tree_revision, window, cx);
                    });
                }
            }
            None => {
                if let Some(palette) = self
                    .command_palette
                    .as_ref()
                    .filter(|palette| palette.read(cx).is_local())
                {
                    palette.update(cx, |palette, cx| palette.refresh(window, cx));
                } else if self.command_palette.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        match choose_tree
            .as_ref()
            .filter(|state| state.kind != zz_protocol::ChooseTreeKind::Windows)
        {
            Some(state) => {
                if self.choose_tree.is_none() {
                    let mux = self.mux.clone();
                    self.choose_tree = Some(cx.new(|cx| ChooseTreeView::new(mux, cx)));
                }
                if let Some(chooser) = &self.choose_tree {
                    chooser.update(cx, |chooser, cx| {
                        chooser.synchronize(state, choose_tree_revision, cx);
                    });
                }
            }
            None => {
                if self.choose_tree.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        match choose_buffer.as_ref() {
            Some(state) => {
                if self.choose_buffer.is_none() {
                    let mux = self.mux.clone();
                    self.choose_buffer = Some(cx.new(|cx| ChooseBufferView::new(mux, cx)));
                }
                if let Some(chooser) = &self.choose_buffer {
                    chooser.update(cx, |chooser, cx| {
                        chooser.synchronize(state, choose_buffer_revision, cx);
                    });
                }
            }
            None => {
                if self.choose_buffer.take().is_some() {
                    self.focused_pane = None;
                }
            }
        }

        if let Some(state) = display_panes.as_ref() {
            self.pane_indicators = state
                .indicators
                .iter()
                .map(|indicator| (indicator.pane, indicator.clone()))
                .collect();
            if self.display_panes.is_none() {
                let mux = self.mux.clone();
                self.display_panes = Some(cx.new(|cx| DisplayPanesView::new(mux, cx)));
            }
            if let Some(overlay) = &self.display_panes {
                overlay.update(cx, |overlay, cx| {
                    overlay.synchronize(display_panes_revision, cx);
                });
            }
        } else {
            self.pane_indicators.clear();
            if self.display_panes.take().is_some() {
                self.focused_pane = None;
            }
        }

        for pane in &wanted_terminals {
            let commands = self
                .mux
                .update(cx, |mux, _| mux.take_terminal_commands(*pane));
            if let Some(terminal) = self.terminals.get(pane) {
                for command in commands {
                    terminal.update(cx, |terminal, cx| {
                        terminal.apply_ui_command(command, window, cx);
                    });
                }
            }
        }

        for pane in &wanted_browsers {
            let commands = self
                .mux
                .update(cx, |mux, _| mux.take_browser_commands(*pane));
            if let Some(browser) = self.browsers.get(pane) {
                for command in commands {
                    browser.update(cx, |browser, cx| browser.apply_command(command, cx));
                }
            }
        }

        self.pickers.retain(|pane, _| wanted_pickers.contains(pane));
        self.empty_picker
            .update(cx, |picker, cx| picker.set_target(chat_target, cx));
        self.terminals
            .retain(|pane, _| wanted_terminals.contains(pane));
        self.terminal_headers.retain(|pane, _| {
            wanted_terminals.contains(pane)
                || wanted_agents.contains(pane)
                || wanted_views.contains(pane)
                || wanted_simulators.contains(pane)
        });
        self.simulators
            .retain(|pane, _| wanted_simulators.contains(pane));
        self.simulator_devices
            .retain(|pane, _| wanted_simulators.contains(pane));
        self.drain_agent_events(cx);
        #[cfg(feature = "agent-pane")]
        self.park_agents(&wanted_agents, &existing, cx);
        self.agents.retain(|pane, _| wanted_agents.contains(pane));
        self.agent_controller.update(cx, |controller, cx| {
            controller.retain_panes(&wanted_agents, &existing, cx);
        });
        self.editors.retain(|pane, _| wanted_editors.contains(pane));
        #[cfg(feature = "agent-pane")]
        {
            let gone = self
                .views
                .keys()
                .filter(|pane| !wanted_views.contains(pane))
                .copied()
                .collect::<Vec<_>>();
            for pane in gone {
                let face = self.pip_faces.borrow_mut().remove(&pane);
                if self.pane_motion.depart(pane, cx)
                    && let (Some(view), Some(face)) = (self.views.get(&pane), face)
                {
                    self.departing.insert(
                        pane,
                        Departing {
                            view: AnyView::from(view.clone()),
                            face,
                        },
                    );
                }
            }
            self.views.retain(|pane, _| wanted_views.contains(pane));
        }
        #[cfg(not(feature = "agent-pane"))]
        let _ = wanted_views;
        let removed_browsers = self
            .browsers
            .keys()
            .filter(|pane| !wanted_browsers.contains(pane))
            .copied()
            .collect::<Vec<_>>();
        for pane in removed_browsers {
            self.controller
                .update(cx, |controller, _| controller.close_pane(pane));
            self.browsers.remove(&pane);
        }

        for session in snapshot
            .sessions
            .iter()
            .filter(|session| Some(session.id) == attached)
        {
            for mux_window in &session.windows {
                let active_window_visible = Some(mux_window.id) == active_window;
                for pane in mux_window.panes.keys() {
                    let covered_by_output = self
                        .command_output
                        .as_ref()
                        .is_some_and(|(output_pane, _)| output_pane == pane);
                    let covered_by_choose_tree = self.choose_tree.is_some();
                    let covered_by_choose_buffer = self.choose_buffer.is_some();
                    let covered_by_settings = route == WorkspaceRoute::Settings;
                    let float = mux_window.floating.iter().find(|float| float.pane == *pane);
                    let visible_in_layout = match float {
                        Some(float) => float.visible || mux_window.zoomed_pane == Some(*pane),
                        None => mux_window.zoomed_pane.is_none_or(|zoomed| zoomed == *pane),
                    };
                    let visible = active_window_visible
                        && visible_in_layout
                        && !covered_by_output
                        && !covered_by_choose_tree
                        && !covered_by_choose_buffer
                        && !covered_by_settings;
                    if let Some(terminal) = self.terminals.get(pane) {
                        terminal.update(cx, |terminal, cx| {
                            terminal.set_visible(visible, cx);
                            terminal.set_floating(
                                float.is_some() && mux_window.zoomed_pane != Some(*pane),
                                cx,
                            );
                        });
                    }
                    if let Some(browser) = self.browsers.get(pane) {
                        browser.update(cx, |browser, cx| {
                            browser.set_visible(visible, window, cx);
                        });
                    }
                    if let Some(agent) = self.agents.get(pane) {
                        agent.update(cx, |agent, cx| agent.set_visible(visible, cx));
                    }
                    if let Some(simulator) = self.simulators.get(pane) {
                        simulator.update(cx, |simulator, cx| {
                            simulator.set_visible(visible, cx);
                            simulator.set_floating(
                                float.is_some() && mux_window.zoomed_pane != Some(*pane),
                                cx,
                            );
                        });
                    }
                }
            }
        }

        let active_pane = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == attached)
            .and_then(|session| {
                let focused_window = snapshot.focused_window_for(session);
                session
                    .windows
                    .iter()
                    .find(|mux_window| mux_window.id == focused_window)
            })
            .map(|mux_window| mux_window.active_pane);
        let persistent_navigator_focused = {
            let sidebar = self.sidebar.read(cx);
            sidebar.is_focused(window)
                && (sidebar.mode() == ChromeMode::Sidebar || sidebar.slideover_open())
        };
        let picker_takes_over = active_pane.is_some_and(|active| {
            self.pickers.contains_key(&active)
                && self.focused_pane.map(|(pane, _)| pane) != Some(active)
        });
        self.audit_pane_focus("pass", window, cx);
        let floating_input = self.menu.is_some()
            || self.confirm.is_some()
            || self
                .command_palette
                .as_ref()
                .is_some_and(|palette| palette.read(cx).is_local());
        let overlay = (route == WorkspaceRoute::App || floating_input)
            .then(|| self.visible_overlay(cx))
            .flatten();
        let previous_overlay = self.focused_overlay;
        let overlay_needs_focus = overlay
            .as_ref()
            .is_some_and(|(kind, _)| self.focused_overlay != Some(*kind));
        self.focused_overlay = overlay.as_ref().map(|(kind, _)| *kind);
        if self.pane_focus_held {
            self.focused_pane = None;
        } else if floating_input {
            if let Some((_, focus)) = overlay
                && overlay_needs_focus
            {
                self.sidebar_focus_owed = persistent_navigator_focused;
                focus.focus(window, cx);
            }
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if route == WorkspaceRoute::Settings {
            if (entering_settings
                || matches!(
                    previous_overlay,
                    Some(OverlayKind::Menu | OverlayKind::Confirm)
                ))
                && let Some(settings) = self.sidebar.read(cx).settings_view()
            {
                let entity = settings.entity_id();
                let focus = settings.read(cx).focus();
                focus.focus(window, cx);
                cx.defer_in(window, move |_, window, cx| {
                    focus.focus(window, cx);
                    ala::App::notify(cx, entity);
                });
            }
            self.sidebar_focus_owed = false;
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if let Some((_, focus)) = overlay {
            if overlay_needs_focus {
                self.sidebar_focus_owed = persistent_navigator_focused;
                focus.focus(window, cx);
            }
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if sidebar_focus_requested {
            log::info!(
                target: "zz::diagnostics::input",
                "sidebar_focus_applied revision={sidebar_focus_revision} was_focused={persistent_navigator_focused}"
            );
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.focus(window, cx));
            self.sidebar_focus_revision = sidebar_focus_revision;
            self.sidebar_focus_owed = false;
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if self.empty_workspace_focus_pending {
            let focus = self.empty_focus(cx);
            focus.focus(window, cx);
            self.empty_workspace_focus_pending = false;
            self.sidebar_focus_owed = false;
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if self.sidebar_focus_owed {
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.refocus(window, cx));
            self.sidebar_focus_owed = false;
            self.pane_focus_owed = false;
            self.focused_pane = None;
        } else if persistent_navigator_focused && !picker_takes_over && !self.pane_focus_owed {
            self.focused_pane = None;
        } else if let Some((pane, entity, focus)) = active_pane.and_then(|pane| {
            self.pane_focus_target(pane, cx)
                .map(|(entity, focus)| (pane, entity, focus))
        }) {
            let stranded = window.focused(cx).is_none();
            if (self.focused_pane != Some((pane, entity)) || stranded)
                && !self.inline_browser_holds(pane, window, cx)
            {
                log::info!(
                    target: "zz::diagnostics::input",
                    "pane_focus_applied pane={pane} previous={:?} stranded={stranded}",
                    self.focused_pane.map(|(pane, _)| pane)
                );
                focus.focus(window, cx);
                cx.defer_in(window, move |view, window, cx| {
                    ala::App::notify(cx, entity);
                    view.audit_pane_focus("applied", window, cx);
                });
            }
            self.pane_focus_owed = false;
            self.focused_pane = Some((pane, entity));
        } else {
            self.focused_pane = None;
        }
        log::trace!(
            target: "zz::diagnostics::app_render",
            "synchronize_panes attached={attached:?} mux_generation={} sessions={} wanted_pickers={} wanted_terminals={} wanted_browsers={} wanted_agents={} wanted_editors={} picker_entities={} terminal_entities={} browser_entities={} agent_entities={} editor_entities={} command_output={} command_palette={} choose_tree={} choose_buffer={} display_panes={} active_window={active_window:?} active_pane={active_pane:?} focused_pane={:?} elapsed_us={}",
            snapshot.generation,
            snapshot.sessions.len(),
            wanted_pickers.len(),
            wanted_terminals.len(),
            wanted_browsers.len(),
            wanted_agents.len(),
            wanted_editors.len(),
            self.pickers.len(),
            self.terminals.len(),
            self.browsers.len(),
            self.agents.len(),
            self.editors.len(),
            self.command_output.is_some(),
            self.command_palette.is_some(),
            self.choose_tree.is_some(),
            self.choose_buffer.is_some(),
            self.display_panes.is_some(),
            self.focused_pane,
            diagnostics::elapsed_us(started),
        );
        self.synchronized_signature = Some(SynchronizeSignature {
            revision,
            sidebar_focused: self.sidebar.read(cx).is_focused(window),
            route,
            chat_mode,
            chat_landing,
            chat_target,
        });
    }

    pub(super) fn drain_gui_requests(&mut self, cx: &mut Context<Self>) {
        if !self.mux.read(cx).has_gui_requests() {
            return;
        }
        let commands = self.mux.update(cx, |mux, _| mux.take_simulator_commands());
        for (pane, command) in commands {
            if let Some(simulator) = self.simulators.get(&pane) {
                simulator.update(cx, |simulator, cx| simulator.command(command, cx));
            } else {
                self.mux.read(cx).respond_to_request(GuiResponse::Error {
                    request_id: command.request_id(),
                    message: format!("{pane} has no simulator view in the attached session"),
                });
            }
        }
        let screenshots = self.mux.update(cx, |mux, _| mux.take_screenshot_requests());
        for (pane, request_id, path) in screenshots {
            if let Some(browser) = self.browsers.get(&pane) {
                browser.update(cx, |browser, cx| browser.screenshot(request_id, path, cx));
            } else {
                self.mux.read(cx).respond_to_request(GuiResponse::Error {
                    request_id,
                    message: format!("{pane} has no live browser view in the attached session"),
                });
            }
        }
    }
}
