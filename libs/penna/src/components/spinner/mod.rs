use std::time::Duration;

use ala::{
    Animation, AnimationExt as _, App, ElementId, Hsla, IntoElement, ParentElement as _,
    RenderOnce, Role, StatefulInteractiveElement as _, Transformation, Window, div, ease_in_out,
    percentage, prelude::FluentBuilder as _,
};

use crate::{AuthoredElement as _, Icon, IconName, Sizable, Size};

const ROTATION_PERIOD: Duration = Duration::from_millis(800);

#[derive(IntoElement)]
pub struct Spinner {
    size: Size,
    color: Option<Hsla>,
    id: Option<ElementId>,
}

impl Spinner {
    #[must_use]
    pub fn new() -> Self {
        Self {
            size: Size::Medium,
            color: None,
            id: None,
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    #[must_use]
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for Spinner {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for Spinner {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let turn = Icon::new(IconName::Loader)
            .with_size(self.size)
            .when_some(self.color, ala::Styled::text_color)
            .with_animation(
                "spinner-rotation",
                Animation::new(ROTATION_PERIOD)
                    .repeat()
                    .with_easing(ease_in_out)
                    .with_max_fps(30.0),
                |this, delta| this.transform(Transformation::rotate(percentage(delta))),
            );
        match self.id {
            Some(id) => div()
                .author_id(id)
                .role(Role::ProgressIndicator)
                .aria_label("Loading")
                .child(turn)
                .into_any_element(),
            None => div().child(turn).into_any_element(),
        }
    }
}
