use ala::{
    AnyElement, App, ElementId, Hsla, IntoElement, ParentElement as _, RenderOnce, Role,
    StatefulInteractiveElement as _, Styled as _, Transformation, Window, div, percentage,
    prelude::FluentBuilder as _,
};

use crate::{
    AuthoredElement as _, Icon, IconName, Sizable, Size,
    foundation::{motion::SPIN, pulse::pulse_phase},
};

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

    #[must_use]
    pub fn covering(self, loading: bool, child: impl IntoElement) -> AnyElement {
        if !loading {
            return child.into_any_element();
        }
        div()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .child(div().opacity(0.0).child(child))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self),
            )
            .into_any_element()
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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let phase = pulse_phase(SPIN, window.current_view(), cx);
        let turn = Icon::new(IconName::Loader)
            .with_size(self.size)
            .when_some(self.color, ala::Styled::text_color)
            .transform(Transformation::rotate(percentage(phase)));
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
