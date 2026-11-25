use std::sync::Arc;

use ala::{
    App, ElementId, FontFeatures, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    PathBuilder, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    canvas, div, point, px, relative,
};

use crate::{
    ActiveTheme as _, Emphasis, Label, PaintState, Space, TextStyle, Tone,
    components::tooltip::Tooltip, h_flex, rems_from_px, v_flex,
};

pub const MAX_SERIES: usize = 4;

const PLOT_HEIGHT: f32 = 160.0;
const GUTTER: f32 = 40.0;
const BAR_MAX: f32 = 24.0;
const BAR_GAP: f32 = 2.0;
const BAR_END: f32 = 4.0;
const LINE: f32 = 2.0;
const DOT: f32 = 8.0;
const SWATCH: f32 = 8.0;
const LABELS: usize = 12;
const TOUCH_LABELS: usize = 6;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChartKind {
    #[default]
    Bar,
    Line,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub name: SharedString,
    pub values: Vec<f64>,
}

impl Series {
    pub fn new(name: impl Into<SharedString>, values: impl IntoIterator<Item = f64>) -> Self {
        Self {
            name: name.into(),
            values: values.into_iter().collect(),
        }
    }
}

#[derive(IntoElement)]
pub struct Chart {
    id: ElementId,
    kind: ChartKind,
    labels: Vec<SharedString>,
    series: Vec<Series>,
    touch: bool,
}

impl Chart {
    pub fn new(
        id: impl Into<ElementId>,
        kind: ChartKind,
        labels: impl IntoIterator<Item = SharedString>,
        series: impl IntoIterator<Item = Series>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            labels: labels.into_iter().collect(),
            series: series.into_iter().take(MAX_SERIES).collect(),
            touch: false,
        }
    }

    #[must_use]
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Scale {
    low: f64,
    high: f64,
    step: f64,
}

impl Scale {
    fn of(values: impl Iterator<Item = f64>) -> Self {
        let (low, high) = values
            .filter(|value| value.is_finite())
            .fold((0.0_f64, 0.0_f64), |(low, high), value| {
                (low.min(value), high.max(value))
            });
        let span = high - low;
        if span <= 0.0 {
            return Self {
                low: 0.0,
                high: 1.0,
                step: 1.0,
            };
        }
        let rough = span / 4.0;
        let magnitude = 10_f64.powf(rough.log10().floor());
        let step = [1.0, 2.0, 2.5, 5.0, 10.0]
            .into_iter()
            .map(|factor| factor * magnitude)
            .find(|step| *step >= rough)
            .unwrap_or(10.0 * magnitude);
        Self {
            low: (low / step).floor() * step,
            high: (high / step).ceil() * step,
            step,
        }
    }

    fn ticks(self) -> Vec<f64> {
        let count = ((self.high - self.low) / self.step).round() as usize;
        (0..=count)
            .map(|index| self.low + index as f64 * self.step)
            .collect()
    }

    fn at(self, value: f64) -> f32 {
        ((value - self.low) / (self.high - self.low)) as f32
    }
}

fn trimmed(value: f64, places: usize) -> String {
    let text = format!("{value:.places$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text.as_str()
    };
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

fn tick(value: f64) -> String {
    let size = value.abs();
    let (scaled, suffix) = if size >= 1e9 {
        (value / 1e9, "B")
    } else if size >= 1e6 {
        (value / 1e6, "M")
    } else if size >= 1e3 {
        (value / 1e3, "k")
    } else {
        (value, "")
    };
    format!("{}{suffix}", trimmed(scaled, 2))
}

fn tabular() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChartCursor {
    pub index: Option<usize>,
}

impl ChartCursor {
    pub fn step(&mut self, delta: isize, len: usize) -> Option<usize> {
        if len == 0 {
            self.index = None;
            return None;
        }
        let next = match self.index {
            Some(at) => (at as isize + delta).clamp(0, len.saturating_sub(1) as isize) as usize,
            None if delta < 0 => len.saturating_sub(1),
            None => 0,
        };
        self.index = Some(next);
        self.index
    }

    pub fn home(&mut self, len: usize) -> Option<usize> {
        self.index = (len > 0).then_some(0);
        self.index
    }

    pub fn end(&mut self, len: usize) -> Option<usize> {
        self.index = len.checked_sub(1);
        self.index
    }
}

fn series_color(index: usize, cx: &App) -> Hsla {
    cx.theme().data_color(index)
}

fn told(label: &SharedString, series: &[Series], index: usize) -> SharedString {
    let value = |one: &Series| {
        one.values
            .get(index)
            .filter(|value| value.is_finite())
            .map_or_else(|| "none".to_owned(), |value| trimmed(*value, 3))
    };
    match series {
        [one] => format!("{label}: {}", value(one)).into(),
        many => {
            let parts: Vec<String> = many
                .iter()
                .map(|one| format!("{} {}", one.name, value(one)))
                .collect();
            format!("{label} · {}", parts.join(" · ")).into()
        }
    }
}

impl RenderOnce for Chart {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let scale = Scale::of(
            self.series
                .iter()
                .flat_map(|one| one.values.iter().copied()),
        );
        let count = self.labels.len();
        let small = if self.touch {
            TextStyle::Body
        } else {
            TextStyle::Caption
        };
        let line = small.line_height_px();
        let rule = cx.theme().divider_color();
        let wash = cx
            .theme()
            .paint(Tone::Neutral, Emphasis::Ghost)
            .fill_at(PaintState::Hover);
        let colors: Vec<Hsla> = (0..self.series.len())
            .map(|index| series_color(index, cx))
            .collect();
        let zero = scale.at(0.0);

        let gutter = div()
            .relative()
            .flex_none()
            .w(rems_from_px(GUTTER))
            .h(rems_from_px(PLOT_HEIGHT))
            .children(scale.ticks().into_iter().map(|value| {
                let top = (1.0 - scale.at(value)) * PLOT_HEIGHT - line / 2.0;
                div()
                    .absolute()
                    .right(Space::S8)
                    .top(rems_from_px(top))
                    .font_features(tabular())
                    .child(Label::new(small, tick(value)).muted())
            }));

        let rules = scale.ticks().into_iter().map(|value| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(relative(1.0 - scale.at(value)))
                .h(px(1.0))
                .bg(rule)
        });

        let bands = (0..count).map(|index| {
            let text = told(&self.labels[index], &self.series, index);
            let band = h_flex()
                .id(("band", index))
                .flex_1()
                .min_w_0()
                .h_full()
                .px(Space::S4)
                .gap(px(BAR_GAP))
                .justify_center()
                .rounded(px(BAR_END))
                .hover(move |this| this.bg(wash))
                .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx));
            if self.kind == ChartKind::Line {
                return band;
            }
            band.children(self.series.iter().zip(&colors).map(|(one, color)| {
                let slot = div()
                    .relative()
                    .flex_1()
                    .h_full()
                    .max_w(rems_from_px(BAR_MAX));
                let Some(value) = one.values.get(index).filter(|value| value.is_finite()) else {
                    return slot;
                };
                let end = scale.at(*value);
                let bar = div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(relative(1.0 - end.max(zero)))
                    .h(relative((end - zero).abs()))
                    .bg(*color);
                slot.child(if end >= zero {
                    bar.rounded_t(px(BAR_END))
                } else {
                    bar.rounded_b(px(BAR_END))
                })
            }))
        });

        let lines = (self.kind == ChartKind::Line).then(|| {
            let drawn: Vec<(Hsla, Vec<Option<f32>>)> = self
                .series
                .iter()
                .zip(&colors)
                .map(|(one, color)| {
                    let heights = (0..count)
                        .map(|index| {
                            one.values
                                .get(index)
                                .filter(|value| value.is_finite())
                                .map(|value| scale.at(*value))
                        })
                        .collect();
                    (*color, heights)
                })
                .collect();
            canvas(
                |_, _, _| (),
                move |bounds, (), window, _| {
                    let band = bounds.size.width / count.max(1) as f32;
                    let spot = |index: usize, height: f32| {
                        point(
                            bounds.origin.x + band * (index as f32 + 0.5),
                            bounds.origin.y + bounds.size.height * (1.0 - height),
                        )
                    };
                    for (color, heights) in &drawn {
                        let mut path = PathBuilder::stroke(px(LINE));
                        let mut run = 0;
                        for (index, height) in heights.iter().enumerate() {
                            match height {
                                Some(height) if run == 0 => {
                                    path.move_to(spot(index, *height));
                                    run = 1;
                                }
                                Some(height) => {
                                    path.line_to(spot(index, *height));
                                    run += 1;
                                }
                                None => run = 0,
                            }
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, *color);
                        }
                        let alone = |index: usize| {
                            let before = index.checked_sub(1).and_then(|at| heights[at]);
                            let after = heights.get(index + 1).copied().flatten();
                            before.is_none() && after.is_none()
                        };
                        for (index, height) in heights.iter().enumerate() {
                            let Some(height) = height.filter(|_| alone(index)) else {
                                continue;
                            };
                            let center = spot(index, height);
                            let radius = px(DOT / 2.0);
                            let mut dot = PathBuilder::fill();
                            dot.move_to(point(center.x + radius, center.y));
                            dot.arc_to(
                                point(radius, radius),
                                px(0.0),
                                false,
                                true,
                                point(center.x - radius, center.y),
                            );
                            dot.arc_to(
                                point(radius, radius),
                                px(0.0),
                                false,
                                true,
                                point(center.x + radius, center.y),
                            );
                            dot.close();
                            if let Ok(dot) = dot.build() {
                                window.paint_path(dot, *color);
                            }
                        }
                    }
                },
            )
            .absolute()
            .inset_0()
        });

        let plot = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h(rems_from_px(PLOT_HEIGHT))
            .children(rules)
            .child(h_flex().absolute().inset_0().children(bands))
            .children(lines);

        let shown = if self.touch { TOUCH_LABELS } else { LABELS };
        let stride = count.div_ceil(shown).max(1);
        let names = h_flex()
            .pl(rems_from_px(GUTTER))
            .children(self.labels.iter().enumerate().map(|(index, label)| {
                let cell = h_flex().flex_1().min_w_0().justify_center();
                if index % stride == 0 {
                    cell.child(Label::new(small, label.clone()).muted().truncate())
                } else {
                    cell
                }
            }));

        let legend = (self.series.len() > 1).then(|| {
            let style = if self.touch {
                TextStyle::TouchSmall
            } else {
                TextStyle::Small
            };
            h_flex()
                .flex_wrap()
                .pl(rems_from_px(GUTTER))
                .gap_x(Space::S12)
                .gap_y(Space::S4)
                .children(self.series.iter().zip(&colors).map(|(one, color)| {
                    h_flex()
                        .gap(Space::S4)
                        .child(
                            div()
                                .flex_none()
                                .size(rems_from_px(SWATCH))
                                .rounded(px(BAR_GAP))
                                .bg(*color),
                        )
                        .child(Label::new(style, one.name.clone()))
                }))
        });

        v_flex()
            .id(self.id)
            .w_full()
            .pt(rems_from_px(line / 2.0))
            .gap(Space::S4)
            .child(h_flex().items_start().child(gutter).child(plot))
            .child(names)
            .children(legend.map(|legend| div().pt(Space::S4).child(legend)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_axis_runs_from_zero_to_a_round_top_in_four_steps_or_so() {
        assert_eq!(
            Scale::of([12.0, 87.0, 40.0].into_iter()).ticks(),
            [0.0, 25.0, 50.0, 75.0, 100.0]
        );
        assert_eq!(
            Scale::of([-3.0, 9.0].into_iter()).ticks(),
            [-5.0, 0.0, 5.0, 10.0]
        );
        assert_eq!(Scale::of(std::iter::empty()).ticks(), [0.0, 1.0]);
        let labels: Vec<String> = Scale::of([0.62, 1800.0].into_iter())
            .ticks()
            .into_iter()
            .map(tick)
            .collect();
        assert_eq!(labels, ["0", "500", "1k", "1.5k", "2k"]);
    }

    #[test]
    fn chart_cursor_clamps_navigation_and_theme_provides_eight_distinct_series_colors() {
        let mut cursor = ChartCursor::default();
        assert_eq!(cursor.index, None);
        assert_eq!(cursor.step(1, 4), Some(0));
        assert_eq!(cursor.step(2, 4), Some(2));
        assert_eq!(cursor.step(10, 4), Some(3));
        assert_eq!(cursor.step(-1, 4), Some(2));
        assert_eq!(cursor.home(4), Some(0));
        assert_eq!(cursor.end(4), Some(3));
        assert_eq!(cursor.step(1, 0), None);

        let mut dark_theme = crate::Theme::default();
        dark_theme.mode = crate::ThemeMode::Dark;
        let dark = dark_theme.data_palette();
        let mut light_theme = crate::Theme::default();
        light_theme.mode = crate::ThemeMode::Light;
        let light = light_theme.data_palette();
        assert_eq!(dark.len(), 8);
        assert_eq!(light.len(), 8);
        for i in 0..8 {
            for j in (i + 1)..8 {
                assert_ne!(crate::to_hex(dark[i]), crate::to_hex(dark[j]));
                assert_ne!(crate::to_hex(light[i]), crate::to_hex(light[j]));
            }
        }
    }
}
