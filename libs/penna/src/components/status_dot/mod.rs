use ala::{
    Animation, AnimationExt as _, App, ElementId, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _,
};
use web_time::Duration;

use crate::{
    ActiveTheme as _, Emphasis, Ground, PaintState, Sizable, Size, Space, StyledExt as _,
    TextStyle, Tone, h_flex, rems_from_px,
};

const PULSE: Duration = Duration::from_millis(1600);

#[derive(IntoElement)]
pub struct StatusDot {
    id: ElementId,
    tone: Tone,
    label: Option<SharedString>,
    pulse: bool,
    size: Size,
    ground: Option<Ground>,
    style: StyleRefinement,
}

impl StatusDot {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, tone: Tone) -> Self {
        Self {
            id: id.into(),
            tone,
            label: None,
            pulse: false,
            size: Size::Medium,
            ground: None,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub const fn pulse(mut self) -> Self {
        self.pulse = true;
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }
}

impl Sizable for StatusDot {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for StatusDot {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn dot_px(size: Size) -> f32 {
    match size.rung() {
        Size::XSmall | Size::Small => 6.0,
        Size::Large | Size::XLarge | Size::Touch | Size::TouchLarge => 10.0,
        _ => 8.0,
    }
}

fn dot_color(tone: Tone, ground: Ground, cx: &App) -> Hsla {
    let theme = cx.theme();
    if tone == Tone::Neutral {
        theme
            .paint_on(ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest)
    } else {
        theme.tone(tone)
    }
}

impl RenderOnce for StatusDot {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let ground = self.ground.unwrap_or_else(|| theme.ground());
        let color = dot_color(self.tone, ground, cx);
        let diameter = rems_from_px(dot_px(self.size));
        let pulse = self.pulse && !cx.reduce_motion();
        let label_color = theme
            .paint_on(ground, Tone::Neutral, Emphasis::Ghost)
            .text_at(PaintState::Rest);
        let text_style = match self.size.rung() {
            Size::XSmall | Size::Small => TextStyle::Caption,
            Size::Large | Size::XLarge => TextStyle::Body,
            _ => TextStyle::Small,
        };
        let marker = div()
            .relative()
            .flex_none()
            .size(diameter)
            .child(div().size_full().rounded_full().bg(color))
            .when(pulse, |host| {
                host.child(div().absolute().inset_0().rounded_full().with_animation(
                    (self.id.clone(), "pulse"),
                    Animation::new(PULSE).repeat().with_max_fps(60.0),
                    move |ring, t| {
                        let expand = rems_from_px(dot_px(self.size) * (0.65 * t));
                        let alpha = (1.0 - t) * 0.45;
                        ring.inset(-expand).bg(color.opacity(alpha))
                    },
                ))
            });
        h_flex()
            .id(self.id)
            .flex_none()
            .items_center()
            .gap(Space::S6)
            .role(Role::Status)
            .when_some(
                self.label.clone(),
                ala::StatefulInteractiveElement::aria_label,
            )
            .refine_style(&self.style)
            .child(marker)
            .when_some(self.label, |el, label| {
                el.child(
                    div()
                        .type_style(text_style, cx)
                        .text_color(label_color)
                        .whitespace_nowrap()
                        .child(label),
                )
            })
    }
}
