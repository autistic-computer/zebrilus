use super::*;

impl BrowserView {
    pub(super) fn on_page_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_page(window, cx);
        self.page_buttons_down |= pointer_button_bit(event.button);
        self.controller.update(cx, |controller, _| {
            if let Some(event) = self.pointer_event(
                event.position,
                PointerPhase::Down,
                Some(event.button),
                event.click_count,
                event.modifiers,
                Some(event.button),
            ) {
                controller.send_pointer(self.pane, self.active_tab, event);
            }
        });
        cx.stop_propagation();
    }

    pub(super) fn on_page_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_page_button(event, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_page_mouse_up_out(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.page_buttons_down != 0 {
            self.release_page_button(event, cx);
        }
    }

    fn release_page_button(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        self.page_buttons_down &= !pointer_button_bit(event.button);
        if let Some(event) = self.pointer_event(
            event.position,
            PointerPhase::Up,
            Some(event.button),
            event.click_count,
            event.modifiers,
            None,
        ) {
            self.controller.update(cx, |controller, _| {
                controller.send_pointer(self.pane, self.active_tab, event);
            });
        }
    }

    pub(super) fn on_page_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !owns_pressed_button(self.page_buttons_down, event.pressed_button) {
            return;
        }
        if let Some(event) = self.pointer_event(
            event.position,
            PointerPhase::Move,
            None,
            0,
            event.modifiers,
            event.pressed_button,
        ) {
            self.controller.update(cx, |controller, _| {
                controller.send_pointer(self.pane, self.active_tab, event);
            });
        }
    }

    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "ala's hover callback supplies &bool"
    )]
    pub(super) fn on_page_hover(
        &mut self,
        hovered: &bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !hovered
            && let Some(event) = self.pointer_event(
                window.mouse_position(),
                PointerPhase::Leave,
                None,
                0,
                ala::Modifiers::default(),
                None,
            )
        {
            self.controller.update(cx, |controller, _| {
                controller.send_pointer(self.pane, self.active_tab, event);
            });
        }
    }

    pub(super) fn on_page_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((x, y)) = self.local_point(event.position) else {
            return;
        };
        let delta = event.delta.pixel_delta(px(20.0));
        let wheel = WheelEvent {
            x,
            y,
            delta_x: rounded_coordinate(delta.x),
            delta_y: rounded_coordinate(delta.y),
            precise: event.delta.precise(),
            modifiers: browser_modifiers(event.modifiers, None, false),
        };
        self.controller.update(cx, |controller, cx| {
            controller.send_wheel(self.pane, self.active_tab, wheel, cx);
        });
        cx.stop_propagation();
    }

    pub(super) fn on_page_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (input, allow_text_input) = browser_surface_key_input(
            self.pane,
            &event.keystroke,
            if event.is_held {
                TerminalKeyAction::Repeat
            } else {
                TerminalKeyAction::Press
            },
        );
        self.mux.read(cx).send_input(input);
        if !allow_text_input {
            cx.stop_propagation();
        }
    }

    pub(super) fn on_page_key_up(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mux
            .read(cx)
            .send_input(InputMessage::BrowserSurfaceKey {
                pane: self.pane,
                input: terminal_key_input(&event.keystroke, TerminalKeyAction::Release),
                text_follows: false,
            });
        cx.stop_propagation();
    }

    fn local_point(&self, point: Point<Pixels>) -> Option<(i32, i32)> {
        self.local_point_with_clamping(point, false)
    }

    fn local_point_with_clamping(&self, point: Point<Pixels>, clamp: bool) -> Option<(i32, i32)> {
        let bounds = self.content_bounds?;
        if !clamp && !bounds.contains(&point) {
            return None;
        }
        Some((
            rounded_coordinate(point.x.clamp(bounds.origin.x, bounds.right()) - bounds.origin.x),
            rounded_coordinate(point.y.clamp(bounds.origin.y, bounds.bottom()) - bounds.origin.y),
        ))
    }

    fn pointer_event(
        &self,
        position: Point<Pixels>,
        phase: PointerPhase,
        button: Option<MouseButton>,
        click_count: usize,
        modifiers: ala::Modifiers,
        pressed: Option<MouseButton>,
    ) -> Option<PointerEvent> {
        let (x, y) =
            self.local_point_with_clamping(position, pointer_needs_clamping(phase, pressed))?;
        Some(PointerEvent {
            x,
            y,
            phase,
            button: button.and_then(pointer_button),
            click_count: i32::try_from(click_count).unwrap_or(i32::MAX),
            modifiers: browser_modifiers(modifiers, pressed, false),
        })
    }
}
