use ala::{
    Animation, AnimationExt as _, App, Div, ElementId, Entity, Hsla, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, prelude::FluentBuilder as _, px, relative,
};
use web_time::Duration;

use crate::{
    ActiveTheme as _, AuthoredElement as _, Emphasis, PaintState, Sizable, Size, Tone,
    components::slider,
    foundation::motion::{Instant, TURN, eased},
};

const SWEEP: Duration = Duration::from_millis(2600);
const PASS: f32 = 2000.0 / 2600.0;
const SEGMENT: f32 = 1.0 / 3.0;
const STILL_HEAD: f32 = 2.0 / 3.0;
const LINE: f32 = 2.0;

#[derive(IntoElement)]
pub struct Progress {
    id: ElementId,
    value: Option<f32>,
    label: SharedString,
    size: Size,
    line: bool,
}

impl Progress {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, value: f32) -> Self {
        Self {
            id: id.into(),
            value: Some(if value.is_nan() {
                0.0
            } else {
                value.clamp(0.0, 1.0)
            }),
            label: "Progress".into(),
            size: Size::Medium,
            line: false,
        }
    }

    #[must_use]
    pub fn indeterminate(id: impl Into<ElementId>) -> Self {
        Self {
            value: None,
            ..Self::new(id, 0.0)
        }
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    #[must_use]
    pub fn line(mut self) -> Self {
        self.line = true;
        self
    }
}

impl Sizable for Progress {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

struct Fill {
    from: f32,
    to: Option<f32>,
    since: Option<Instant>,
}

fn shown(value: Option<f32>, fill: &Entity<Fill>, window: &Window, cx: &mut App) -> Option<f32> {
    let Some(value) = value else {
        fill.update(cx, |fill, _| fill.to = None);
        return None;
    };
    let Fill { from, to, since } = *fill.read(cx);
    let Some(to) = to else {
        fill.update(cx, |fill, _| {
            *fill = Fill {
                from: value,
                to: Some(value),
                since: None,
            };
        });
        return Some(value);
    };
    let at = from + (to - from) * eased(since, Duration::ZERO, TURN, window, cx);
    if (to - value).abs() <= f32::EPSILON {
        return Some(at);
    }
    let now = cx.background_executor().now();
    fill.update(cx, |fill, _| {
        *fill = Fill {
            from: at,
            to: Some(value),
            since: Some(now),
        };
    });
    eased(Some(now), Duration::ZERO, TURN, window, cx);
    Some(at)
}

fn span(head: f32) -> Option<(f32, f32)> {
    let start = (head - SEGMENT).clamp(0.0, 1.0);
    let end = head.clamp(0.0, 1.0);
    (end > start).then_some((start, end))
}

fn segment(head: f32, color: Hsla, line: bool) -> Option<Div> {
    span(head).map(|(start, end)| {
        div()
            .absolute()
            .top_0()
            .h_full()
            .left(relative(start))
            .w(relative(end - start))
            .when(!line, ala::Styled::rounded_full)
            .bg(color)
    })
}

impl RenderOnce for Progress {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let fill = window.use_keyed_state(self.id.clone(), cx, |_, _| Fill {
            from: 0.0,
            to: None,
            since: None,
        });
        let shown = shown(self.value, &fill, window, cx);
        let theme = cx.theme();
        let track = theme
            .paint(Tone::Neutral, Emphasis::Soft)
            .fill_at(PaintState::Active);
        let accent = theme.paint(Tone::Accent, Emphasis::Solid).fill;
        let still = cx.reduce_motion();
        let line = self.line;
        let height = if line {
            px(LINE)
        } else {
            px(slider::rail(self.size))
        };
        div()
            .author_id(self.id)
            .role(Role::ProgressIndicator)
            .aria_label(self.label)
            .when_some(self.value, |this, value| {
                this.aria_min_numeric_value(0.0)
                    .aria_max_numeric_value(1.0)
                    .aria_numeric_value(f64::from(value))
                    .aria_value(format!("{:.0}%", value * 100.0))
            })
            .relative()
            .flex_none()
            .w_full()
            .h(height)
            .when(!line, |bar| bar.rounded_full().bg(track))
            .map(|bar| match shown {
                Some(shown) => bar.child(
                    div()
                        .h_full()
                        .w(relative(shown))
                        .when(!line, ala::Styled::rounded_full)
                        .bg(accent),
                ),
                None if still => bar.children(segment(STILL_HEAD, accent, line)),
                None => bar.child(div().absolute().inset_0().with_animation(
                    "progress-sweep",
                    Animation::new(SWEEP).repeat().with_max_fps(60.0),
                    move |sweep, delta| {
                        sweep.children(segment(delta / PASS * (1.0 + SEGMENT), accent, line))
                    },
                )),
            })
    }
}

#[cfg(test)]
mod tests {
    use ala::{Context, Render, TestAppContext, VisualTestContext, div, px};

    use super::*;

    struct Fixture;

    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(300.0))
                .flex()
                .flex_col()
                .child(Progress::new("download", 1.3).label("Downloading"))
                .child(Progress::indeterminate("verify").label("Verifying"))
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    #[ala::test]
    fn a_bar_announces_its_clamped_value_and_an_indeterminate_one_none(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|window, _| {
            window.set_a11y_forced(true);
            Fixture
        });
        draw(cx);
        cx.update(|window, _| {
            let tree = window.a11y_tree().unwrap();
            let node = |label| {
                tree.nodes
                    .iter()
                    .map(|(_, node)| node)
                    .find(|node| {
                        node.role() == Role::ProgressIndicator && node.label() == Some(label)
                    })
                    .unwrap()
            };
            let download = node("Downloading");
            assert_eq!(download.numeric_value(), Some(1.0));
            assert_eq!(download.value(), Some("100%"));
            let verify = node("Verifying");
            assert_eq!(verify.numeric_value(), None);
            assert_eq!(verify.value(), None);
        });
    }
}
