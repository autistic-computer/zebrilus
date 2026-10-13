use std::rc::Rc;

use zz_gpui::{
    App, ElementId, IntoElement, RenderOnce, Role, SharedString, Window, div, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, PaintState, Size, StyledExt as _, Tone, h_flex,
    tooltip::Tooltip,
};

type ChangeHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct DiscreteSlider {
    id: ElementId,
    labels: Vec<SharedString>,
    selected: usize,
    label: SharedString,
    on_change: ChangeHandler,
}

impl DiscreteSlider {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        labels: Vec<SharedString>,
        selected: usize,
        on_change: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            labels,
            selected,
            label: label.into(),
            on_change: Rc::new(on_change),
        }
    }
}

impl RenderOnce for DiscreteSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = self.labels.len();
        let selected = self.selected.min(count.saturating_sub(1));
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle());
        let focus = state.read(cx).clone();
        let focused = focus.is_focused(window);
        let foreground = cx.theme().foreground;
        let filled = cx.theme().paint(Tone::Primary, Emphasis::Solid);
        let track = cx.theme().paint(Tone::Neutral, Emphasis::Soft);
        let value = self.labels.get(selected).cloned().unwrap_or_default();
        let mut pills = h_flex()
            .flex_1()
            .min_w_0()
            .max_w(px(
                count as f32 * 24.0 + count.saturating_sub(1) as f32 * 4.0
            ))
            .gap_1();
        for (index, label) in self.labels.iter().enumerate() {
            let change = self.on_change.clone();
            let focus = focus.clone();
            let tooltip = label.clone();
            let selector = format!("{}:pill-{index}", self.id);
            let group = SharedString::from(selector.clone());
            let (fill, hover) = if index <= selected {
                (filled.fill, filled.hover)
            } else {
                (track.fill_at(PaintState::Active), track.hover)
            };
            pills = pills.child(
                h_flex()
                    .id(index)
                    .group(group.clone())
                    .debug_selector(move || selector.clone())
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .flex_1()
                    .min_w_0()
                    .h(Size::Medium.control_h())
                    .cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                    .child(
                        div()
                            .w_full()
                            .h(px(6.0))
                            .rounded(cx.theme().control_radius())
                            .bg(fill)
                            .group_hover(group, move |this| this.bg(hover))
                            .when(focused && index == selected, |this| {
                                this.focus_ring(&filled)
                            }),
                    )
                    .on_click(move |_, window, cx| {
                        focus.focus(window, cx);
                        change(index, window, cx);
                    }),
            );
        }
        h_flex()
            .id(self.id)
            .role(Role::Slider)
            .aria_label(self.label.clone())
            .aria_value(value.clone())
            .track_focus(&focus)
            .w_full()
            .gap_2()
            .text_xs()
            .line_height(px(16.0))
            .on_key_down(move |event, window, cx| {
                let next = match event.keystroke.key.as_str() {
                    "left" | "down" => selected.saturating_sub(1),
                    "right" | "up" => (selected + 1).min(count.saturating_sub(1)),
                    "home" => 0,
                    "end" => count.saturating_sub(1),
                    _ => return,
                };
                cx.stop_propagation();
                if count > 0 && next != selected {
                    (self.on_change)(next, window, cx);
                }
            })
            .child(
                div()
                    .flex_none()
                    .text_color(foreground.muted())
                    .child(self.label),
            )
            .child(pills)
            .child(
                div()
                    .flex_none()
                    .w(px(52.0))
                    .text_color(foreground)
                    .child(value),
            )
    }
}
