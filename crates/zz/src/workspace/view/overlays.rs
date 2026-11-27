use super::*;

impl AppView {
    fn route_menu_pointer(
        &mut self,
        kind: MenuPointerKind,
        buttons: u8,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.menu.clone() else {
            return;
        };
        menu.update(cx, |menu, cx| {
            menu.pointer(kind, buttons, position, cx);
        });
        cx.stop_propagation();
    }

    pub(super) fn on_menu_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.route_menu_pointer(
            MenuPointerKind::Press,
            crate::command::menu::press_buttons(event.button),
            event.position,
            cx,
        );
    }

    pub(super) fn on_menu_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.route_menu_pointer(
            MenuPointerKind::Release,
            crate::command::menu::RELEASE_BUTTONS,
            event.position,
            cx,
        );
    }

    pub(super) fn menu_overlay(
        &self,
        origin: Point<Pixels>,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let menu = self.menu.as_ref()?;
        let frame = menu_frame(
            menu.read(cx).state(),
            origin,
            self.pane_canvas_bounds.get().size,
            window.scale_factor(),
        );
        Some(
            anchored()
                .position_mode(AnchoredPositionMode::Local)
                .position(frame.bounds.origin)
                .snap_to_window_with_margin(px(8.0))
                .child(menu.clone())
                .into_any_element(),
        )
    }

    pub(super) fn confirm_overlay(&self, origin: Point<Pixels>, cx: &App) -> Option<AnyElement> {
        let confirm = self.confirm.as_ref()?;
        let canvas = self.pane_canvas_bounds.get().size;
        let prompt = &confirm.read(cx).state().prompt;
        let width = px((display_width(prompt).saturating_mul(8).saturating_add(32)) as f32)
            .max(px(180.0))
            .min((canvas.width - px(24.0)).max(px(180.0)));
        let height = px(48.0);
        let left = origin.x + ((canvas.width - width) / 2.0).max(Pixels::ZERO);
        let top = origin.y + ((canvas.height - height) / 2.0).max(Pixels::ZERO);
        Some(
            div()
                .absolute()
                .left(left)
                .top(top)
                .w(width)
                .h(height)
                .debug_selector(|| "confirm-before".to_owned())
                .child(FloatingSurface::new(
                    "confirm-before-surface",
                    confirm.clone(),
                    cx,
                ))
                .into_any_element(),
        )
    }
}

fn menu_frame(
    state: &MenuState,
    origin: Point<Pixels>,
    canvas: Size<Pixels>,
    scale: f32,
) -> PopupFrame {
    floating_frame(
        state.left,
        state.top,
        state.width,
        state.height,
        state.client_columns,
        state.client_rows,
        state.cell_width_px,
        state.cell_height_px,
        state.border_lines != PopupBorderLines::None,
        origin,
        canvas,
        scale,
    )
}
