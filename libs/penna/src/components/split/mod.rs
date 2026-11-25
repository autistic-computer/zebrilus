use std::rc::Rc;

use ala::{
    AnyElement, App, Bounds, CursorStyle, ElementId, InteractiveElement as _, IntoElement,
    MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, RenderOnce, Role,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, canvas, div,
    prelude::FluentBuilder as _, px, relative,
};

use crate::{ActiveTheme as _, AuthoredElement as _, StyledExt as _, h_flex, v_flex};

const SASH_HIT: f32 = 8.0;

type ResizeHandler = Rc<dyn Fn(f32, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplitAxis {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitState {
    pub ratio: f32,
    pub default_ratio: f32,
    pub min_ratio: f32,
    pub max_ratio: f32,
    pub dragging: bool,
    pub bounds: Bounds<Pixels>,
}

impl SplitState {
    #[must_use]
    pub fn new(ratio: f32, min_ratio: f32, max_ratio: f32) -> Self {
        let min = min_ratio.clamp(0.05, 0.9);
        let max = max_ratio.clamp(min + 0.05, 0.95);
        let clamped = ratio.clamp(min, max);
        Self {
            ratio: clamped,
            default_ratio: clamped,
            min_ratio: min,
            max_ratio: max,
            dragging: false,
            bounds: Bounds::default(),
        }
    }

    pub fn set_ratio(&mut self, next: f32) -> f32 {
        if next.is_finite() {
            self.ratio = next.clamp(self.min_ratio, self.max_ratio);
        }
        self.ratio
    }

    pub fn reset(&mut self) -> f32 {
        self.ratio = self.default_ratio;
        self.ratio
    }

    pub fn ratio_from_pointer(&self, axis: SplitAxis, pos_x: Pixels, pos_y: Pixels) -> Option<f32> {
        let (offset, extent) = match axis {
            SplitAxis::Horizontal => (pos_x - self.bounds.origin.x, self.bounds.size.width),
            SplitAxis::Vertical => (pos_y - self.bounds.origin.y, self.bounds.size.height),
        };
        let total = f32::from(extent);
        if total <= 1.0 {
            return None;
        }
        Some((f32::from(offset) / total).clamp(self.min_ratio, self.max_ratio))
    }
}

#[derive(IntoElement)]
pub struct Split {
    id: ElementId,
    axis: SplitAxis,
    initial_ratio: f32,
    min_ratio: f32,
    max_ratio: f32,
    first: Option<AnyElement>,
    second: Option<AnyElement>,
    on_resize: Option<ResizeHandler>,
    style: StyleRefinement,
}

impl Split {
    #[must_use]
    pub fn horizontal(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            axis: SplitAxis::Horizontal,
            initial_ratio: 0.5,
            min_ratio: 0.15,
            max_ratio: 0.85,
            first: None,
            second: None,
            on_resize: None,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn vertical(id: impl Into<ElementId>) -> Self {
        Self {
            axis: SplitAxis::Vertical,
            ..Self::horizontal(id)
        }
    }

    #[must_use]
    pub const fn ratio(mut self, ratio: f32) -> Self {
        self.initial_ratio = ratio;
        self
    }

    #[must_use]
    pub const fn limits(mut self, min: f32, max: f32) -> Self {
        self.min_ratio = min;
        self.max_ratio = max;
        self
    }

    #[must_use]
    pub fn first(mut self, element: impl IntoElement) -> Self {
        self.first = Some(element.into_any_element());
        self
    }

    #[must_use]
    pub fn second(mut self, element: impl IntoElement) -> Self {
        self.second = Some(element.into_any_element());
        self
    }

    #[must_use]
    pub fn on_resize(mut self, handler: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_resize = Some(Rc::new(handler));
        self
    }
}

impl Styled for Split {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Split {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let initial = self.initial_ratio;
        let min = self.min_ratio;
        let max = self.max_ratio;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| {
            SplitState::new(initial, min, max)
        });
        let snapshot = *state.read(cx);
        let axis = self.axis;
        let theme = cx.theme();
        let divider = theme.border();
        let accent = theme.accent;
        let cursor = match axis {
            SplitAxis::Horizontal => CursorStyle::ResizeLeftRight,
            SplitAxis::Vertical => CursorStyle::ResizeUpDown,
        };
        let on_resize = self.on_resize;

        let bounds_state = state.clone();
        let measure = canvas(
            move |bounds, _, cx| {
                bounds_state.update(cx, |st, _| st.bounds = bounds);
            },
            |_, (), _, _| {},
        )
        .absolute()
        .inset_0();

        let down_state = state.clone();
        let move_state = state.clone();
        let up_state = state.clone();
        let move_cb = on_resize.clone();
        let reset_cb = on_resize;

        let sash = div()
            .author_id((self.id.clone(), "sash"))
            .role(Role::Splitter)
            .aria_label("Resize panes")
            .aria_min_numeric_value(f64::from(snapshot.min_ratio * 100.0))
            .aria_max_numeric_value(f64::from(snapshot.max_ratio * 100.0))
            .aria_numeric_value(f64::from(snapshot.ratio * 100.0))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor(cursor)
            .when(axis == SplitAxis::Horizontal, |el| {
                el.w(px(SASH_HIT)).h_full()
            })
            .when(axis == SplitAxis::Vertical, |el| {
                el.h(px(SASH_HIT)).w_full()
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                if event.click_count >= 2 {
                    let ratio = down_state.update(cx, |st, cx| {
                        st.dragging = false;
                        let r = st.reset();
                        cx.notify();
                        r
                    });
                    if let Some(cb) = &reset_cb {
                        cb(ratio, window, cx);
                    }
                    return;
                }
                down_state.update(cx, |st, cx| {
                    st.dragging = true;
                    cx.notify();
                });
            })
            .child(
                div()
                    .when(axis == SplitAxis::Horizontal, |line| {
                        line.w(px(if snapshot.dragging { 2.0 } else { 1.0 }))
                            .h_full()
                    })
                    .when(axis == SplitAxis::Vertical, |line| {
                        line.h(px(if snapshot.dragging { 2.0 } else { 1.0 }))
                            .w_full()
                    })
                    .bg(if snapshot.dragging { accent } else { divider }),
            );

        let first_pane = div()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .when(axis == SplitAxis::Horizontal, |el| {
                el.w(relative(snapshot.ratio)).h_full()
            })
            .when(axis == SplitAxis::Vertical, |el| {
                el.h(relative(snapshot.ratio)).w_full()
            })
            .children(self.first);

        let second_pane = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .children(self.second);

        let host = match axis {
            SplitAxis::Horizontal => h_flex(),
            SplitAxis::Vertical => v_flex(),
        };

        host.id(self.id)
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .refine_style(&self.style)
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                if !move_state.read(cx).dragging {
                    return;
                }
                window.set_window_cursor_style(cursor);
                let next = move_state.update(cx, |st, cx| {
                    let ratio = st.ratio_from_pointer(axis, event.position.x, event.position.y)?;
                    st.set_ratio(ratio);
                    cx.notify();
                    Some(st.ratio)
                });
                if let (Some(ratio), Some(cb)) = (next, &move_cb) {
                    cb(ratio, window, cx);
                }
            })
            .on_mouse_up(MouseButton::Left, move |_: &MouseUpEvent, _, cx| {
                if up_state.read(cx).dragging {
                    up_state.update(cx, |st, cx| {
                        st.dragging = false;
                        cx.notify();
                    });
                }
            })
            .child(measure)
            .child(first_pane)
            .child(sash)
            .child(second_pane)
    }
}

#[cfg(test)]
mod tests {
    use ala::{point, size};

    use super::*;

    #[test]
    fn split_clamps_pointer_drag_and_resets_to_default() {
        let mut st = SplitState::new(0.4, 0.2, 0.8);
        st.bounds = Bounds::new(point(px(100.0), px(50.0)), size(px(500.0), px(300.0)));
        assert_eq!(
            st.ratio_from_pointer(SplitAxis::Horizontal, px(350.0), px(100.0)),
            Some(0.5)
        );
        assert_eq!(
            st.ratio_from_pointer(SplitAxis::Horizontal, px(120.0), px(100.0)),
            Some(0.2)
        );
        assert_eq!(
            st.ratio_from_pointer(SplitAxis::Vertical, px(200.0), px(330.0)),
            Some(0.8)
        );
        st.set_ratio(0.7);
        assert_eq!(st.ratio, 0.7);
        assert_eq!(st.reset(), 0.4);
    }
}
