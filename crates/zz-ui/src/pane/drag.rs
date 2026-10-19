use std::{rc::Rc, time::Duration};

use zz_gpui::{
    AnyElement, App, Bounds, Context, CursorStyle, Div, Entity, IntoElement, Pixels, Point, Render,
    Stateful, Window, div, prelude::*, px,
};
use zz_protocol::PaneId;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DropPreviewFrame {
    pub bounds: Bounds<Pixels>,
    pub opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropPreview {
    pub from: DropPreviewFrame,
    pub to: DropPreviewFrame,
    pub sequence: u64,
    pub duration: Duration,
}

impl DropPreview {
    pub fn at(self, delta: f32) -> DropPreviewFrame {
        DropPreviewFrame {
            bounds: lerp_bounds(self.from.bounds, self.to.bounds, delta),
            opacity: self.from.opacity + (self.to.opacity - self.from.opacity) * delta,
        }
    }
}

fn lerp_bounds(from: Bounds<Pixels>, to: Bounds<Pixels>, delta: f32) -> Bounds<Pixels> {
    Bounds::new(
        zz_gpui::point(
            lerp_pixels(from.origin.x, to.origin.x, delta),
            lerp_pixels(from.origin.y, to.origin.y, delta),
        ),
        zz_gpui::size(
            lerp_pixels(from.size.width, to.size.width, delta),
            lerp_pixels(from.size.height, to.size.height, delta),
        ),
    )
}

fn lerp_pixels(from: Pixels, to: Pixels, delta: f32) -> Pixels {
    from + (to - from) * delta
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneDrag {
    pub pane: PaneId,
    pub requires_prefix: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipGrab {
    pub pane: PaneId,
}

struct PipGrabPreview;

impl Render for PipGrabPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub struct PaneDragPreview {
    pane: PaneId,
    title: String,
    grab: Point<Pixels>,
}

impl Render for PaneDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.grab.x + px(12.0))
            .pt(self.grab.y + px(12.0))
            .child(super::pane_drag_chip(
                self.pane.to_string(),
                self.title.clone(),
                cx,
            ))
    }
}

pub fn pane_drag_preview(
    pane: PaneId,
    title: String,
    grab: Point<Pixels>,
    cx: &mut App,
) -> Entity<PaneDragPreview> {
    cx.new(|_| PaneDragPreview { pane, title, grab })
}

/// What dragging a pane by its header needs: the pane, the title its drag
/// chip shows, whether it may move now, and what to do when a drag starts.
pub struct PaneDragSource {
    pane: PaneId,
    title: String,
    enabled: bool,
    floating: bool,
    on_start: Rc<dyn Fn(&PaneDrag, &mut Window, &mut App)>,
    touch: Option<AnyElement>,
}

impl PaneDragSource {
    pub fn new(
        pane: PaneId,
        title: impl Into<String>,
        enabled: bool,
        on_start: impl Fn(&PaneDrag, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            pane,
            title: title.into(),
            enabled,
            floating: false,
            on_start: Rc::new(on_start),
            touch: None,
        }
    }

    #[must_use]
    pub const fn floating(mut self, floating: bool) -> Self {
        self.floating = floating;
        self
    }

    /// Lays `overlay` over the dragged element while enabled, for a client
    /// that drags panes with its own touch handling.
    #[must_use]
    pub fn touch(mut self, overlay: impl IntoElement) -> Self {
        self.touch = Some(overlay.into_any_element());
        self
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// `element`, dragging the pane while the source is enabled.
    pub fn attach(self, element: Stateful<Div>) -> Stateful<Div> {
        if !self.enabled {
            return element;
        }
        let Self {
            pane,
            title,
            floating,
            on_start,
            touch,
            ..
        } = self;
        let element = element
            .relative()
            .children(touch)
            .cursor(CursorStyle::OpenHand);
        if floating {
            return element.on_drag(PipGrab { pane }, |_, _, _, cx| cx.new(|_| PipGrabPreview));
        }
        element.on_drag(
            PaneDrag {
                pane,
                requires_prefix: false,
            },
            move |drag, grab, window, cx| {
                on_start(drag, window, cx);
                window.defer(cx, |window, cx| {
                    cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
                });
                pane_drag_preview(drag.pane, title.clone(), grab, cx)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zz_gpui::{Modifiers, MouseButton, TestAppContext, point};

    struct DragTest {
        enabled: bool,
        started: Vec<PaneDrag>,
        dropped: Vec<PaneDrag>,
    }

    impl Render for DragTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let view = cx.entity();
            div()
                .flex()
                .w(px(240.0))
                .h(px(100.0))
                .child(
                    PaneDragSource::new(PaneId(3), "Session", self.enabled, move |drag, _, cx| {
                        view.update(cx, |view, _| view.started.push(*drag));
                    })
                    .attach(
                        div()
                            .id("test-handle")
                            .debug_selector(|| "pane-drag-handle".into())
                            .size(px(24.0)),
                    ),
                )
                .child(
                    div().id("test-drop").w(px(200.0)).h(px(100.0)).on_drop(
                        cx.listener(|view, drag: &PaneDrag, _, _| view.dropped.push(*drag)),
                    ),
                )
        }
    }

    #[zz_gpui::test]
    fn handle_starts_without_prefix_and_drops_after_a_single_move(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|_, _| DragTest {
            enabled: true,
            started: Vec::new(),
            dropped: Vec::new(),
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let start = cx.debug_bounds("pane-drag-handle").unwrap().center();
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        assert!(!cx.update(|_, cx| cx.has_active_drag()));
        let target = point(px(150.0), px(12.0));
        cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        assert!(cx.update(|_, cx| cx.has_active_drag()));
        let payload = PaneDrag {
            pane: PaneId(3),
            requires_prefix: false,
        };
        assert_eq!(
            view.read_with(cx, |view, _| view.started.clone()),
            [payload]
        );
        cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        assert!(!cx.update(|_, cx| cx.has_active_drag()));
        assert_eq!(
            view.read_with(cx, |view, _| view.dropped.clone()),
            [payload]
        );

        view.update(cx, |view, cx| {
            view.enabled = false;
            cx.notify();
        });
        cx.run_until_parked();
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::default());
        assert!(!cx.update(|_, cx| cx.has_active_drag()));
        cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
        assert_eq!(
            view.read_with(cx, |view, _| view.started.clone()),
            [payload]
        );
    }
}
