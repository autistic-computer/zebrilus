use std::{cell::RefCell, rc::Rc};

use zz_client::{
    MenuBox, MenuKeyResult, MenuPointerKind,
    floating::{FloatCells, float_pixels, float_title},
    resolve_menu_mouse,
};
use zz_gpui::{
    AnchoredPositionMode, AnyElement, App, Bounds, Context, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollWheelEvent, Window, anchored, div, point, prelude::*, px,
    size,
};
use zz_protocol::{
    FloatingPaneSnapshot, InputMessage, MenuState, PaneBorderLines, PopupBorderLines,
};
use zz_ui::{
    ActiveTheme as _,
    command::floating::{
        MenuHits, RELEASE_BUTTONS, WHEEL_BUTTONS, confirm_prompt, display_menu, floating_frame,
        menu_enter, menu_hit_cell, menu_press_buttons,
    },
    pane::FloatingSurface,
};

use super::AppShell;
use crate::preferences::Preferences;

impl AppShell {
    pub(super) fn floating_canvas_size(&self, window: &Window, cx: &App) -> zz_gpui::Size<Pixels> {
        let viewport = window.fully_visible_bounds().size;
        size(
            (viewport.width
                - px(if self.inline_sidebar(window) {
                    self.sidebar_width(window)
                } else {
                    0.0
                }))
            .max(px(0.0)),
            (viewport.height
                - if self.settings.is_none() && !self.inline_sidebar(window) {
                    cx.theme().title_bar_height()
                } else {
                    px(0.0)
                })
            .max(px(0.0)),
        )
    }

    pub(super) fn floating_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let core = &self.connection.read(cx).core;
        let canvas = self.floating_canvas_size(window, cx);
        if let Some(state) = core.menu().cloned() {
            let frame = floating_frame(
                state.left,
                state.top,
                state.width,
                state.height,
                state.client_columns,
                state.client_rows,
                state.cell_width_px,
                state.cell_height_px,
                state.border_lines != PopupBorderLines::None,
                point(px(0.0), px(0.0)),
                canvas,
                window.scale_factor(),
            );
            let hits = Rc::new(RefCell::new(MenuHits::default()));
            let surface = menu_enter(display_menu(
                &state,
                self.menu_selection,
                window.viewport_size(),
                &hits,
                cx,
            ));
            let state = Rc::new(state);
            let (press_state, press_hits) = (state.clone(), hits.clone());
            let (release_state, release_hits) = (state.clone(), hits.clone());
            let (move_state, move_hits) = (state.clone(), hits.clone());
            return Some(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .capture_any_mouse_down(cx.listener(
                        move |this, event: &MouseDownEvent, _, cx| {
                            this.menu_pointer(
                                &press_state,
                                &press_hits.borrow(),
                                (MenuPointerKind::Press, menu_press_buttons(event.button)),
                                event.position,
                                cx,
                            );
                            cx.stop_propagation();
                        },
                    ))
                    .capture_any_mouse_up(cx.listener(move |this, event: &MouseUpEvent, _, cx| {
                        this.menu_pointer(
                            &release_state,
                            &release_hits.borrow(),
                            (MenuPointerKind::Release, RELEASE_BUTTONS),
                            event.position,
                            cx,
                        );
                        cx.stop_propagation();
                    }))
                    .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                        let (kind, buttons) = event
                            .pressed_button
                            .map_or((MenuPointerKind::Motion, RELEASE_BUTTONS), |button| {
                                (MenuPointerKind::Drag, menu_press_buttons(button))
                            });
                        this.menu_pointer(
                            &move_state,
                            &move_hits.borrow(),
                            (kind, buttons),
                            event.position,
                            cx,
                        );
                        cx.stop_propagation();
                    }))
                    .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                        this.menu_pointer(
                            &state,
                            &hits.borrow(),
                            (MenuPointerKind::Wheel, WHEEL_BUTTONS),
                            event.position,
                            cx,
                        );
                        cx.stop_propagation();
                    }))
                    .child(
                        anchored()
                            .position_mode(AnchoredPositionMode::Local)
                            .position(frame.bounds.origin)
                            .snap_to_window_with_margin(px(8.0))
                            .child(surface),
                    )
                    .into_any_element(),
            );
        }
        let state = core.confirm()?;
        let width = px((zz_protocol::display_width(&state.prompt)
            .saturating_mul(8)
            .saturating_add(32)) as f32)
        .max(px(180.0))
        .min((canvas.width - px(24.0)).max(px(180.0)));
        let height = px(48.0);
        Some(
            div()
                .absolute()
                .left(((canvas.width - width) / 2.0).max(px(0.0)))
                .top(((canvas.height - height) / 2.0).max(px(0.0)))
                .w(width)
                .h(height)
                .child(FloatingSurface::new(
                    "web-confirm-before-surface",
                    confirm_prompt(
                        "web-confirm-before-input",
                        state.prompt.clone(),
                        cx.theme().mono_font_family.clone(),
                    ),
                    cx,
                ))
                .into_any_element(),
        )
    }

    fn menu_pointer(
        &mut self,
        state: &MenuState,
        hits: &MenuHits,
        (kind, buttons): (MenuPointerKind, u8),
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !state.mouse_keys && kind == MenuPointerKind::Motion {
            return;
        }
        let (column, row) = menu_hit_cell(state, hits, position);
        match resolve_menu_mouse(
            state,
            self.menu_selection,
            MenuBox {
                left: state.left,
                top: state.top,
                width: state.width,
                items: state.items.len(),
            },
            kind,
            buttons,
            column,
            row,
        ) {
            MenuKeyResult::Action(action) => self.send_input(InputMessage::Menu { action }, cx),
            MenuKeyResult::Select(selected) => {
                if self.menu_selection != selected {
                    self.menu_selection = selected;
                    cx.notify();
                }
            }
            MenuKeyResult::Consumed => {}
        }
    }
}

impl AppShell {
    pub(super) fn cell_size(
        &self,
        active: &zz_protocol::WindowSnapshot,
        cx: &Context<Self>,
    ) -> (f32, f32) {
        std::iter::once(active.active_pane)
            .chain(self.terminals.keys().copied())
            .find_map(|pane| self.terminals.get(&pane))
            .map_or((8.0, 18.0), |terminal| {
                let (width, height) = terminal.read(cx).cell_size();
                (f32::from(width), f32::from(height))
            })
    }

    pub(super) fn float_layer(
        &self,
        active: &zz_protocol::WindowSnapshot,
        floats: &[zz_protocol::FloatingPaneSnapshot],
        panes: &mut std::collections::HashMap<zz_protocol::PaneId, AnyElement>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let cell = self.cell_size(active, cx);
        let canvas = self.float_canvas_bounds.get().size;
        let mut layer = Vec::new();
        for float in floats {
            let Some(content) = panes.remove(&float.pane) else {
                continue;
            };
            if let Some(modal) = active.modal.filter(|modal| modal.pane == float.pane) {
                let pane = modal.pane;
                layer.push(
                    div()
                        .id(("web-modal-scrim", pane.0))
                        .absolute()
                        .inset_0()
                        .occlude()
                        .bg(cx.theme().scrim)
                        .on_mouse_down(
                            zz_gpui::MouseButton::Left,
                            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                if modal.close_on_click {
                                    this.command(
                                        "kill-pane",
                                        vec!["-t".into(), pane.to_string()],
                                        cx,
                                    );
                                }
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                );
            }
            let Some(placed) = float_placement(float, cell, canvas) else {
                continue;
            };
            let title = active
                .panes
                .get(&float.pane)
                .filter(|_| float.border_status.is_on())
                .map(|pane| float_title(&pane.border_status_text).left)
                .unwrap_or_default();
            let border_color = zz_ui::pane::pane_border_color(active.active_pane == float.pane, cx);
            let frame = placed.frame;
            let inset = placed.content.origin - frame.origin;
            layer.push(
                div()
                    .absolute()
                    .left(frame.origin.x)
                    .top(frame.origin.y)
                    .w(frame.size.width)
                    .h(frame.size.height)
                    .child(
                        FloatingSurface::new(("web-float", float.pane.0), content, cx)
                            .title(title)
                            .content_inset(inset.x, inset.y)
                            .edge_color(border_color)
                            .bordered(float.border_lines != PaneBorderLines::None),
                    )
                    .into_any_element(),
            );
        }
        (!layer.is_empty()).then(|| {
            div()
                .absolute()
                .inset_0()
                .overflow_hidden()
                .children(layer)
                .into_any_element()
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PaneFrame {
    pub header: bool,
    pub framed: bool,
    pub radius: f32,
}

pub(super) fn pane_frame(
    terminal: bool,
    floating: bool,
    preferences: &Preferences,
    pane_radius: f32,
) -> PaneFrame {
    let gaps = preferences.gaps && !floating;
    PaneFrame {
        header: terminal && !floating,
        framed: gaps,
        radius: if gaps { pane_radius } else { 0.0 },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FloatPlacement {
    pub frame: Bounds<Pixels>,
    pub content: Bounds<Pixels>,
}

#[allow(
    clippy::cast_precision_loss,
    reason = "window cell offsets are far below f32's exact integer range"
)]
pub(super) fn float_placement(
    float: &FloatingPaneSnapshot,
    cell: (f32, f32),
    canvas: zz_gpui::Size<Pixels>,
) -> Option<FloatPlacement> {
    let bordered = float.border_lines != PaneBorderLines::None;
    if canvas.width > Pixels::ZERO
        && canvas.height > Pixels::ZERO
        && float_pixels(
            FloatCells::from(float),
            bordered,
            cell,
            (f32::from(canvas.width), f32::from(canvas.height)),
        )
        .is_none()
    {
        return None;
    }
    let pad = i32::from(bordered);
    let rect = |x: i32, y: i32, width: i32, height: i32| {
        Bounds::new(
            point(px(x as f32 * cell.0), px(y as f32 * cell.1)),
            size(px(width as f32 * cell.0), px(height as f32 * cell.1)),
        )
    };
    Some(FloatPlacement {
        frame: rect(
            float.xoff - pad,
            float.yoff - pad,
            i32::from(float.sx) + 2 * pad,
            i32::from(float.sy) + 2 * pad,
        ),
        content: rect(
            float.xoff,
            float.yoff,
            i32::from(float.sx),
            i32::from(float.sy),
        ),
    })
}

#[cfg(any(target_os = "ios", test))]
pub(super) fn native_panes_shown(
    tiled: impl IntoIterator<Item = (zz_protocol::PaneId, Option<Bounds<Pixels>>)>,
    floats: &[(zz_protocol::PaneId, FloatPlacement)],
    modal: Option<zz_protocol::PaneId>,
) -> std::collections::BTreeSet<zz_protocol::PaneId> {
    let covered = |bounds: Bounds<Pixels>, front: &[(zz_protocol::PaneId, FloatPlacement)]| {
        front
            .iter()
            .any(|(_, placed)| placed.frame.intersects(&bounds))
    };
    tiled
        .into_iter()
        .filter(|(_, bounds)| bounds.map_or(floats.is_empty(), |bounds| !covered(bounds, floats)))
        .map(|(pane, _)| pane)
        .chain(
            floats
                .iter()
                .enumerate()
                .filter(|(index, (_, placed))| !covered(placed.content, &floats[..*index]))
                .map(|(_, (pane, _))| *pane),
        )
        .filter(|pane| modal.is_none_or(|modal| modal == *pane))
        .collect()
}

#[cfg(test)]
mod tests {
    use zz_gpui::{Bounds, point, px, size};
    use zz_protocol::{FloatingPaneSnapshot, PaneBorderLines, PaneBorderStatus, PaneId};

    use super::{FloatPlacement, float_placement, native_panes_shown, pane_frame};
    use crate::preferences::Preferences;

    fn float(pane: u64, xoff: i32, yoff: i32, sx: u16, sy: u16) -> FloatingPaneSnapshot {
        FloatingPaneSnapshot {
            pane: PaneId(pane),
            xoff,
            yoff,
            sx,
            sy,
            visible: true,
            border_lines: PaneBorderLines::Single,
            border_status: PaneBorderStatus::Off,
        }
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<zz_gpui::Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    #[test]
    fn a_floating_terminal_keeps_its_whole_box_for_the_grid() {
        let preferences = Preferences {
            gaps: true,
            ..Preferences::default()
        };
        assert_eq!(
            pane_frame(true, true, &preferences, 10.0),
            super::PaneFrame {
                header: false,
                framed: false,
                radius: 0.0,
            }
        );
        let tiled = pane_frame(true, false, &preferences, 10.0);
        assert!(tiled.header);
        assert!(tiled.framed);
        assert_eq!(tiled.radius, 10.0);
        assert!(!pane_frame(false, false, &preferences, 10.0).header);
        let gapless = Preferences {
            gaps: false,
            ..preferences
        };
        assert_eq!(pane_frame(true, false, &gapless, 10.0).radius, 0.0);
    }

    #[test]
    fn a_float_past_the_left_edge_is_cropped_not_reshaped() {
        let canvas = size(px(400.0), px(560.0));
        let placed = float_placement(&float(1, -3, 4, 10, 6), (8.0, 16.0), canvas).unwrap();
        assert_eq!(
            placed,
            FloatPlacement {
                frame: rect(-32.0, 48.0, 96.0, 128.0),
                content: rect(-24.0, 64.0, 80.0, 96.0),
            }
        );
        let inset = placed.content.origin - placed.frame.origin;
        assert_eq!((inset.x, inset.y), (px(8.0), px(16.0)));
        assert_eq!(
            placed.frame.size.width - placed.content.size.width,
            inset.x * 2.0
        );
        assert_eq!(
            float_placement(&float(1, 60, 4, 10, 6), (8.0, 16.0), canvas),
            None
        );
        assert!(
            float_placement(&float(1, 60, 4, 10, 6), (8.0, 16.0), size(px(0.0), px(0.0))).is_some()
        );
    }

    #[test]
    fn a_float_hides_only_the_native_browsers_it_covers() {
        let cell = (8.0, 16.0);
        let canvas = size(px(800.0), px(480.0));
        let left = (PaneId(1), Some(rect(0.0, 0.0, 400.0, 480.0)));
        let right = (PaneId(2), Some(rect(400.0, 0.0, 400.0, 480.0)));
        let front = (
            PaneId(3),
            float_placement(&float(3, 5, 5, 20, 10), cell, canvas).unwrap(),
        );
        let behind_front = (
            PaneId(4),
            float_placement(&float(4, 10, 8, 20, 10), cell, canvas).unwrap(),
        );
        let apart = (
            PaneId(5),
            float_placement(&float(5, 70, 20, 20, 5), cell, canvas).unwrap(),
        );
        assert_eq!(
            native_panes_shown([left, right], &[], None),
            [PaneId(1), PaneId(2)].into()
        );
        assert_eq!(
            native_panes_shown([left, right], &[front], None),
            [PaneId(2), PaneId(3)].into()
        );
        assert_eq!(
            native_panes_shown([left, right], &[front, behind_front, apart], None),
            [PaneId(3), PaneId(5)].into()
        );
        let border_over_left = (
            PaneId(6),
            float_placement(&float(6, 50, 5, 10, 5), cell, canvas).unwrap(),
        );
        assert!(!border_over_left.1.content.intersects(&left.1.unwrap()));
        assert_eq!(
            native_panes_shown([left, right], &[border_over_left], None),
            [PaneId(6)].into()
        );
        assert_eq!(
            native_panes_shown([(PaneId(1), None)], &[front], None),
            [PaneId(3)].into()
        );
    }

    #[test]
    fn a_modal_hides_every_native_browser_but_its_own() {
        let cell = (8.0, 16.0);
        let canvas = size(px(800.0), px(480.0));
        let left = (PaneId(1), Some(rect(0.0, 0.0, 400.0, 480.0)));
        let right = (PaneId(2), Some(rect(400.0, 0.0, 400.0, 480.0)));
        let modal = (
            PaneId(3),
            float_placement(&float(3, 5, 5, 20, 10), cell, canvas).unwrap(),
        );
        let behind = (
            PaneId(4),
            float_placement(&float(4, 60, 2, 10, 4), cell, canvas).unwrap(),
        );
        assert!(!modal.1.frame.intersects(&right.1.unwrap()));
        assert!(!modal.1.frame.intersects(&behind.1.frame));
        assert_eq!(
            native_panes_shown([left, right], &[modal], None),
            [PaneId(2), PaneId(3)].into()
        );
        assert_eq!(
            native_panes_shown([left, right], &[modal], Some(PaneId(3))),
            [PaneId(3)].into()
        );
        assert_eq!(
            native_panes_shown([left], &[modal, behind], None),
            [PaneId(3), PaneId(4)].into()
        );
        assert_eq!(
            native_panes_shown([left], &[modal, behind], Some(PaneId(3))),
            [PaneId(3)].into()
        );
        assert_eq!(
            native_panes_shown([left, right], &[behind], Some(PaneId(3))),
            [].into()
        );
    }
}
