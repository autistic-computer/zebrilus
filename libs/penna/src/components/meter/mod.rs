use ala::{
    App, ElementId, IntoElement, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px, relative,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Colorize as _, Emphasis, PaintState, Sizable, Size,
    Space, StyledExt as _, TextStyle, Tone, components::slider, h_flex, v_flex,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterThresholds {
    pub warn: f32,
    pub danger: f32,
}

impl Default for MeterThresholds {
    fn default() -> Self {
        Self {
            warn: 0.75,
            danger: 0.90,
        }
    }
}

#[derive(IntoElement)]
pub struct Meter {
    id: ElementId,
    value: f32,
    min: f32,
    max: f32,
    label: Option<SharedString>,
    value_label: Option<SharedString>,
    tone: Option<Tone>,
    thresholds: MeterThresholds,
    size: Size,
    style: StyleRefinement,
}

impl Meter {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, value: f32) -> Self {
        Self {
            id: id.into(),
            value,
            min: 0.0,
            max: 1.0,
            label: None,
            value_label: None,
            tone: None,
            thresholds: MeterThresholds::default(),
            size: Size::Medium,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub const fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    #[must_use]
    pub const fn thresholds(mut self, warn: f32, danger: f32) -> Self {
        self.thresholds = MeterThresholds { warn, danger };
        self
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn value_label(mut self, label: impl Into<SharedString>) -> Self {
        self.value_label = Some(label.into());
        self
    }

    #[must_use]
    pub const fn tone(mut self, tone: Tone) -> Self {
        self.tone = Some(tone);
        self
    }

    #[must_use]
    pub fn ratio(&self) -> f32 {
        let span = self.max - self.min;
        if !span.is_finite() || span <= 0.0 || !self.value.is_finite() {
            return 0.0;
        }
        ((self.value - self.min) / span).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn resolved_tone(&self) -> Tone {
        if let Some(tone) = self.tone {
            return tone;
        }
        let ratio = self.ratio();
        if ratio >= self.thresholds.danger {
            Tone::Danger
        } else if ratio >= self.thresholds.warn {
            Tone::Warning
        } else {
            Tone::Success
        }
    }
}

impl Sizable for Meter {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for Meter {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Meter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ratio = self.ratio();
        let tone = self.resolved_tone();
        let theme = cx.theme();
        let track = theme
            .paint(Tone::Neutral, Emphasis::Soft)
            .fill_at(PaintState::Active);
        let fill = if tone == Tone::Neutral {
            theme.foreground
        } else {
            theme.tone(tone)
        };
        let height = px(slider::rail(self.size));
        let aria = self
            .label
            .clone()
            .unwrap_or_else(|| SharedString::from("Meter"));
        let announced = self
            .value_label
            .clone()
            .unwrap_or_else(|| SharedString::from(format!("{:.0}%", ratio * 100.0)));
        let has_header = self.label.is_some() || self.value_label.is_some();

        v_flex()
            .w_full()
            .gap(Space::S6)
            .refine_style(&self.style)
            .when(has_header, |root| {
                root.child(
                    h_flex()
                        .w_full()
                        .items_baseline()
                        .justify_between()
                        .gap(Space::S8)
                        .when_some(self.label, |row, label| {
                            row.child(
                                div()
                                    .type_style(TextStyle::Small, cx)
                                    .text_color(theme.foreground)
                                    .child(label),
                            )
                        })
                        .when_some(self.value_label, |row, val| {
                            row.child(
                                div()
                                    .font_family(theme.mono_font_family.clone())
                                    .type_style(TextStyle::Caption, cx)
                                    .text_color(theme.foreground.muted())
                                    .child(val),
                            )
                        }),
                )
            })
            .child(
                div()
                    .author_id(self.id)
                    .role(Role::Meter)
                    .aria_label(aria)
                    .aria_min_numeric_value(f64::from(self.min))
                    .aria_max_numeric_value(f64::from(self.max))
                    .aria_numeric_value(f64::from(self.value.clamp(self.min, self.max)))
                    .aria_value(announced)
                    .relative()
                    .w_full()
                    .h(height)
                    .rounded_full()
                    .bg(track)
                    .overflow_hidden()
                    .child(div().h_full().w(relative(ratio)).rounded_full().bg(fill)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_clamps_ratio_and_transitions_tones_at_thresholds() {
        let low = Meter::new("cpu", 42.0).range(0.0, 100.0);
        assert_eq!(low.ratio(), 0.42);
        assert_eq!(low.resolved_tone(), Tone::Success);

        let warn = Meter::new("mem", 80.0).range(0.0, 100.0);
        assert_eq!(warn.resolved_tone(), Tone::Warning);

        let hot = Meter::new("ctx", 210.0).range(0.0, 200.0);
        assert_eq!(hot.ratio(), 1.0);
        assert_eq!(hot.resolved_tone(), Tone::Danger);

        let explicit = Meter::new("disk", 95.0)
            .range(0.0, 100.0)
            .tone(Tone::Accent);
        assert_eq!(explicit.resolved_tone(), Tone::Accent);
    }
}
