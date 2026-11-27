use super::*;

impl AppView {
    pub(super) fn pane_focus_target(
        &self,
        pane: PaneId,
        cx: &App,
    ) -> Option<(EntityId, FocusHandle)> {
        if let Some(picker) = self.pickers.get(&pane) {
            Some((picker.entity_id(), picker.read(cx).focus().clone()))
        } else if let Some(agent) = self.agents.get(&pane) {
            Some((agent.entity_id(), agent.read(cx).focus(cx)))
        } else if let Some(terminal) = self.terminals.get(&pane) {
            Some((terminal.entity_id(), terminal.read(cx).focus()))
        } else if let Some(browser) = self.browsers.get(&pane) {
            Some((browser.entity_id(), browser.read(cx).pane_focus_handle(cx)))
        } else if let Some(editor) = self.editors.get(&pane) {
            Some((editor.entity_id(), editor.read(cx).focus(cx)))
        } else if let Some(simulator) = self.simulators.get(&pane) {
            Some((simulator.entity_id(), simulator.read(cx).focus()))
        } else {
            self.view_focus_target(pane, cx)
        }
    }

    #[cfg(feature = "agent-pane")]
    fn view_focus_target(&self, pane: PaneId, cx: &App) -> Option<(EntityId, FocusHandle)> {
        self.views
            .get(&pane)
            .map(|view| (view.entity_id(), view.read(cx).focus()))
    }

    #[cfg(not(feature = "agent-pane"))]
    fn view_focus_target(&self, _: PaneId, _: &App) -> Option<(EntityId, FocusHandle)> {
        None
    }

    pub(super) fn inline_browser_holds(&self, pane: PaneId, window: &Window, cx: &App) -> bool {
        self.inline_browser.read(cx).holds_focus(window)
            && window.context_stack().iter().any(|context| {
                context
                    .get(PANE_KEY_CONTEXT_ID)
                    .map(ala::SharedString::as_ref)
                    == Some(pane.0.to_string().as_str())
            })
    }

    pub(super) fn audit_pane_focus(&self, phase: &str, window: &Window, cx: &App) {
        let Some((pane, entity)) = self.focused_pane else {
            return;
        };
        let target = self.pane_focus_target(pane, cx);
        let observed = window.focused(cx);
        let contexts = window.context_stack();
        let holds_keyboard = (target.as_ref().is_some_and(|(current, focus)| {
            *current == entity && observed.as_ref() == Some(focus)
        }) && contexts.iter().any(|context| {
            context
                .get(PANE_KEY_CONTEXT_ID)
                .map(ala::SharedString::as_ref)
                == Some(pane.0.to_string().as_str())
        })) || self.inline_browser_holds(pane, window, cx);
        if holds_keyboard {
            log::debug!(
                target: "zz::diagnostics::input",
                "pane_focus_live phase={phase} pane={pane} entity={entity:?}"
            );
            return;
        }
        log::warn!(
            target: "zz::diagnostics::input",
            "pane_focus_lost phase={phase} pane={pane} entity={entity:?} view={:?} wanted={:?} window_focus={observed:?} contexts={contexts:?}",
            target.as_ref().map(|(entity, _)| *entity),
            target.as_ref().map(|(_, focus)| focus),
        );
    }

    pub(crate) const fn hold_pane_focus(&mut self) {
        self.pane_focus_held = true;
    }

    pub(crate) fn release_pane_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pane_focus_held = false;
        self.synchronized_signature = None;
        self.synchronize_panes(window, cx);
        cx.notify();
    }

    pub(super) fn focus_active_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_focus_held || self.sidebar.read(cx).route() == WorkspaceRoute::Settings {
            return;
        }
        if self.empty_workspace_visible {
            let focus = self.empty_focus(cx);
            focus.focus(window, cx);
            self.focused_pane = None;
            return;
        }
        let active_pane = {
            let mux = self.mux.read(cx);
            let snapshot = mux.snapshot();
            snapshot
                .sessions
                .iter()
                .find(|session| Some(session.id) == mux.attached_session())
                .and_then(|session| {
                    let focused_window = snapshot.focused_window_for(session);
                    session
                        .windows
                        .iter()
                        .find(|mux_window| mux_window.id == focused_window)
                })
                .map(|mux_window| mux_window.active_pane)
        };
        let target = active_pane.and_then(|pane| {
            self.pane_focus_target(pane, cx)
                .map(|(entity, focus)| (pane, entity, focus))
        });
        log::info!(
            target: "zz::diagnostics::input",
            "focus_active_pane pane={active_pane:?} focused={}",
            target.is_some()
        );
        self.focused_pane = target.map(|(pane, entity, focus)| {
            focus.focus(window, cx);
            (pane, entity)
        });
    }

    pub(super) fn visible_overlay(&self, cx: &App) -> Option<(OverlayKind, FocusHandle)> {
        if let Some(menu) = &self.menu {
            Some((OverlayKind::Menu, menu.read(cx).focus().clone()))
        } else if let Some(confirm) = &self.confirm {
            Some((OverlayKind::Confirm, confirm.read(cx).focus().clone()))
        } else if let Some(palette) = &self.command_palette {
            Some((OverlayKind::CommandPalette, palette.read(cx).focus(cx)))
        } else if let Some(chooser) = &self.choose_buffer {
            Some((OverlayKind::ChooseBuffer, chooser.read(cx).focus().clone()))
        } else if let Some(chooser) = &self.choose_tree {
            Some((OverlayKind::ChooseTree, chooser.read(cx).focus().clone()))
        } else if let Some(panes) = &self.display_panes {
            Some((OverlayKind::DisplayPanes, panes.read(cx).focus().clone()))
        } else {
            self.command_output
                .as_ref()
                .map(|(pane, output)| (OverlayKind::CommandOutput(*pane), output.read(cx).focus()))
        }
    }

    pub(super) fn empty_focus(&self, cx: &App) -> FocusHandle {
        if self.sidebar.read(cx).chat_mode() && self.mux.read(cx).has_hosts() {
            self.empty_picker.read(cx).focus().clone()
        } else {
            self.new_session.read(cx).focus().clone()
        }
    }

    pub(super) fn on_pane_pointer_focus(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.mux.read(cx).execute(pane_select_command(pane));
    }

    pub(super) fn on_pane_click(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.mux.read(cx).execute(pane_select_command(pane));
        self.dismiss_armed_prefix(pane, cx);
        cx.notify();
    }

    pub(super) fn dismiss_armed_prefix(&self, pane: PaneId, cx: &App) {
        let escape = Keystroke {
            modifiers: ala::Modifiers::default(),
            key: "escape".to_owned(),
            key_char: None,
        };
        for action in [TerminalKeyAction::Press, TerminalKeyAction::Release] {
            self.mux.read(cx).send_input(InputMessage::Key {
                pane,
                input: terminal_key_input(&escape, action),
                text_follows: false,
            });
        }
    }
}
