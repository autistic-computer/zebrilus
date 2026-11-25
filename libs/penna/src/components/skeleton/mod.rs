use ala::{
    Animation, AnimationExt as _, App, ElementId, IntoElement, Length, RenderOnce, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _,
};
use web_time::Duration;

use crate::{
    ActiveTheme as _, Emphasis, PaintState, StyledExt as _, TextStyle, Tone, rems_from_px,
};

const SHIMMER: Duration = Duration::from_millis(1400);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkeletonShape {
    #[default]
    Line,
    Block,
    Circle,
}

#[derive(IntoElement)]
pub struct Skeleton {
    id: ElementId,
    shape: SkeletonShape,
    width: Option<Length>,
    height: Option<Length>,
    style: StyleRefinement,
}

impl Skeleton {
    #[must_use]
    pub fn line(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            shape: SkeletonShape::Line,
            width: None,
            height: None,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn block(id: impl Into<ElementId>) -> Self {
        Self {
            shape: SkeletonShape::Block,
            ..Self::line(id)
        }
    }

    #[must_use]
    pub fn circle(id: impl Into<ElementId>, size: impl Into<Length>) -> Self {
        let size = size.into();
        Self {
            shape: SkeletonShape::Circle,
            width: Some(size),
            height: Some(size),
            ..Self::line(id)
        }
    }

    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    #[must_use]
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = Some(height.into());
        self
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Skeleton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let base = theme
            .paint(Tone::Neutral, Emphasis::Soft)
            .fill_at(PaintState::Active);
        let default_h = match self.shape {
            SkeletonShape::Line => TextStyle::Small.line_height(),
            SkeletonShape::Block => rems_from_px(64.0),
            SkeletonShape::Circle => rems_from_px(28.0),
        };
        let box_el = div()
            .flex_none()
            .when_some(self.width, Styled::w)
            .when(self.width.is_none(), Styled::w_full)
            .h(self.height.unwrap_or_else(|| default_h.into()))
            .map(|el| match self.shape {
                SkeletonShape::Circle => el.rounded_full(),
                SkeletonShape::Line => el.rounded(rems_from_px(4.0)),
                SkeletonShape::Block => el.rounded(theme.radius),
            })
            .bg(base)
            .refine_style(&self.style);

        if cx.reduce_motion() {
            return box_el.into_any_element();
        }
        box_el
            .with_animation(
                self.id,
                Animation::new(SHIMMER).repeat().with_max_fps(30.0),
                move |el, t| {
                    let wave = (t * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                    el.opacity(0.55 + 0.45 * wave)
                },
            )
            .into_any_element()
    }
}
