use super::*;

impl AppView {
    pub(crate) fn on_window_shortcut_modifiers(
        &mut self,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let held = window_shortcut_modifiers(cx) == Some(modifiers);
        if !held {
            self.hide_window_shortcuts(cx);
            return;
        }
        if self.window_shortcut_timer.is_some() {
            return;
        }
        self.window_shortcut_timer = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(WINDOW_SHORTCUT_HINT_DELAY)
                .await;
            view.update(cx, |view, cx| {
                view.sidebar
                    .update(cx, |sidebar, cx| sidebar.show_window_shortcuts(true, cx));
            })
            .ok();
        }));
    }

    pub(super) fn hide_window_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.window_shortcut_timer = None;
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.show_window_shortcuts(false, cx));
    }

    pub(super) fn intercept_keystroke(
        &mut self,
        event: &ala::KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.window_handle() != self.window_handle
            || matches!(
                event.keystroke.key.as_str(),
                "shift" | "control" | "alt" | "platform" | "function"
            )
        {
            return;
        }
        if !(event.is_held && self.mux.read(cx).prefix_armed()) {
            self.hide_which_key(cx);
        }
        if !self.sidebar.read(cx).window_shortcuts_shown() {
            self.window_shortcut_timer = None;
        }
        if self.reconcile_dialog_prefix(window, cx) {
            return;
        }
        let keystroke = &event.keystroke;
        let overlay_open = self.overlay_open(cx);
        if !overlay_open
            && keystroke.key == "escape"
            && self
                .pane_drag
                .as_ref()
                .is_some_and(|drag| !drag.requires_prefix)
        {
            self.pane_drag = None;
            cx.stop_active_drag(window);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if !overlay_open
            && self.dialog_prefix_cancel_pending.is_some()
            && !keystroke.modifiers.platform
            && !keystroke.modifiers.function
        {
            cx.stop_propagation();
            return;
        }
        let input = terminal_key_input(
            keystroke,
            if event.is_held {
                TerminalKeyAction::Repeat
            } else {
                TerminalKeyAction::Press
            },
        );
        let (prefix, active) = {
            let mux = self.mux.read(cx);
            let active = mux.active_pane().and_then(|id| {
                let snapshot = mux.snapshot();
                snapshot
                    .sessions
                    .iter()
                    .filter(|session| Some(session.id) == mux.attached_session())
                    .flat_map(|session| &session.windows)
                    .find_map(|window| window.panes.get(&id))
                    .map(|pane| {
                        (
                            id,
                            match pane.kind {
                                PaneKindSnapshot::Terminal => SurfaceKind::Terminal,
                                PaneKindSnapshot::Browser(_) => SurfaceKind::Browser,
                                _ => SurfaceKind::Other,
                            },
                        )
                    })
            });
            (
                PrefixView {
                    armed: mux.prefix_armed(),
                    claimed: !overlay_open
                        && !keystroke.modifiers.platform
                        && !keystroke.modifiers.function
                        && mux.claims_prefix_input(&input),
                },
                active,
            )
        };
        let armed = prefix.armed;
        if prefix.claimed && armed && keystroke.key == "escape" && self.pane_drag.take().is_some() {
            cx.stop_active_drag(window);
            cx.notify();
        }
        if active != self.input_router.active_pane() {
            if let Some((pane, kind)) = active {
                self.input_router
                    .event(InputEvent::SetActivePane(pane, kind));
            } else {
                self.input_router.event(InputEvent::Detached);
            }
        }
        self.input_router.set_capture(if overlay_open {
            None
        } else {
            self.mux.read(cx).modal_capture()
        });
        let disposition = self.input_router.key(&input, prefix);
        let effects = self.input_router.drain_effects();
        if prefix.claimed && disposition == Disposition::Native && active.is_none() {
            log::warn!(
                target: "zz::diagnostics::input",
                "prefix_key_dropped keystroke={keystroke} armed={armed} reason=no_active_pane"
            );
        }
        if prefix.claimed
            && event.is_held
            && effects.is_empty()
            && let Some((pane, _)) = active
        {
            log::debug!(
                target: "zz::diagnostics::input",
                "prefix_key_autorepeat_swallowed keystroke={keystroke} armed={armed} pane={pane}"
            );
        }
        for effect in effects {
            match effect {
                Effect::Chrome(action) => self.run_chrome_action(action, window, cx),
                Effect::ForwardKey { pane, input } => {
                    if armed
                        && zz_client::prefix_keys::which_key_delay(
                            config::resolved_config(cx).which_key_delay.value,
                        )
                        .is_some()
                        && zz_client::prefix_keys::is_all_keys_input(
                            self.mux.read(cx).prefix_bindings(),
                            &input,
                        )
                    {
                        self.input_router.suppress_release(
                            input
                                .unshifted_codepoint
                                .map_or(input.key, zz_terminal::KeyCode::Character),
                        );
                        self.which_key = which_key::sheet(self.mux.read(cx), "prefix", true);
                        cx.notify();
                    } else if armed
                        && let Some(action) = self.prefix_chrome_action(keystroke, &input, cx)
                    {
                        self.input_router.suppress_release(
                            input
                                .unshifted_codepoint
                                .map_or(input.key, zz_terminal::KeyCode::Character),
                        );
                        self.mux.update(cx, |mux, _| mux.send_prefix_cancel());
                        self.run_chrome_action(action, window, cx);
                    } else {
                        log::info!(
                            target: "zz::diagnostics::input",
                            "prefix_key_forwarded keystroke={keystroke} armed={armed} pane={pane}"
                        );
                        self.mux.read(cx).send_input(InputMessage::Key {
                            pane,
                            input,
                            text_follows: false,
                        });
                    }
                }
                Effect::RequestFocus(_) => {}
            }
        }
        if disposition == Disposition::Consumed {
            cx.stop_propagation();
        }
    }

    fn prefix_chrome_action(
        &self,
        keystroke: &Keystroke,
        input: &zz_terminal::KeyInput,
        cx: &App,
    ) -> Option<ChromeAction> {
        let mux = self.mux.read(cx);
        if mux.key_table() != Some(("prefix", false))
            || zz_client::prefix_keys::binds_input(mux.prefix_bindings(), input)
        {
            return None;
        }
        crate::keymap::resolve(cx, zz_client::PREFIX_TABLE, keystroke)
    }

    fn run_chrome_action(
        &mut self,
        action: ChromeAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action: Box<dyn ala::Action> = match action {
            ChromeAction::OpenCommandPalette => {
                self.open_command_palette(None, window, cx);
                return;
            }
            ChromeAction::SelectWindow(position) => {
                if self
                    .sidebar
                    .update(cx, |sidebar, cx| sidebar.open_window_shortcut(position, cx))
                {
                    self.focus_active_pane(window, cx);
                }
                return;
            }
            ChromeAction::NewSession => Box::new(crate::menus::NewSession),
            ChromeAction::NewWindow => Box::new(crate::menus::NewWindow),
            ChromeAction::SplitRight => Box::new(crate::menus::SplitRight),
            ChromeAction::SplitDown => Box::new(crate::menus::SplitDown),
            ChromeAction::Detach => Box::new(crate::menus::Detach),
            ChromeAction::ToggleSidebar => Box::new(crate::menus::ToggleSidebar),
            ChromeAction::OpenSettings => Box::new(config::settings::OpenSettings),
            _ => return,
        };
        window.dispatch_action(action, cx);
    }

    fn overlay_open(&self, cx: &App) -> bool {
        self.menu.is_some()
            || self.confirm.is_some()
            || self.command_palette.as_ref().is_some_and(|palette| {
                palette.read(cx).is_local() || palette.read(cx).is_window_chooser()
            })
            || self.sidebar.read(cx).route() == WorkspaceRoute::Settings
    }

    pub(super) fn which_key_blocked(&self, window: &Window, cx: &App) -> bool {
        self.overlay_open(cx)
            || self.visible_overlay(cx).is_some()
            || self.pane_drag.is_some()
            || cx.has_active_drag()
            || window
                .root::<penna::Root>()
                .flatten()
                .is_some_and(|root| root.read(cx).has_active_dialog())
    }

    fn hide_which_key(&mut self, cx: &mut Context<Self>) {
        self.which_key_timer = None;
        self.which_key_generation = self.which_key_generation.wrapping_add(1);
        if self.which_key.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn key_table_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide_which_key(cx);
        let Some((table, false)) = self.mux.read(cx).key_table() else {
            return;
        };
        let table = table.to_owned();
        let Some(delay) = zz_client::prefix_keys::which_key_delay(
            config::resolved_config(cx).which_key_delay.value,
        ) else {
            return;
        };
        let generation = self.which_key_generation;
        self.which_key_timer = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(delay).await;
            view.update_in(cx, |view, window, cx| {
                view.show_which_key(generation, &table, window, cx);
            })
            .ok();
        }));
    }

    fn show_which_key(
        &mut self,
        generation: u64,
        table: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.which_key_generation {
            return;
        }
        self.which_key_timer = None;
        if self.mux.read(cx).key_table() != Some((table, false))
            || self.which_key_blocked(window, cx)
        {
            return;
        }
        self.which_key = which_key::sheet(self.mux.read(cx), table, false);
        cx.notify();
    }

    pub(super) fn reconcile_dialog_prefix(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let active = window
            .root::<penna::Root>()
            .flatten()
            .is_some_and(|root| root.read(cx).has_active_dialog());
        let acknowledged = self.mux.read(cx).prefix_cancelled_request();
        if self
            .dialog_prefix_cancel_pending
            .is_some_and(|pending| acknowledged.is_some_and(|acknowledged| acknowledged >= pending))
        {
            self.dialog_prefix_cancel_pending = None;
        }
        if !active {
            self.dialog_prefix_cancel_sent = false;
        } else if !self.dialog_prefix_cancel_sent
            && let Some(request_id) = self.mux.update(cx, |mux, _| mux.send_prefix_cancel())
        {
            self.dialog_prefix_cancel_pending = Some(request_id);
            self.dialog_prefix_cancel_sent = true;
        }
        active
    }

    pub fn on_claim_key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let input = terminal_key_input(&event.keystroke, TerminalKeyAction::Release);
        let disposition = self.input_router.key(&input, PrefixView::default());
        for effect in self.input_router.drain_effects() {
            if let Effect::ForwardKey { pane, input } = effect {
                self.mux.read(cx).send_input(InputMessage::Key {
                    pane,
                    input,
                    text_follows: false,
                });
            }
        }
        if disposition == Disposition::Consumed {
            cx.stop_propagation();
        }
    }
}
