use std::cmp::Ordering;

use ala::{
    App, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    Icon, IconName, Label, PaintState, Sizable as _, Space, StyledExt as _, Surface, SurfaceRole,
    TextStyle, Tone, h_flex, rems_from_px,
};

const DOT: f32 = 20.0;
const LINE: f32 = 28.0;
const LINE_WIDTH: f32 = 2.0;
const CHECK: f32 = 12.0;

#[derive(IntoElement)]
pub struct Steps {
    labels: Vec<SharedString>,
    current: usize,
}

impl Steps {
    #[must_use]
    pub fn new(labels: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        Self {
            labels: labels.into_iter().map(Into::into).collect(),
            current: 0,
        }
    }

    #[must_use]
    pub fn current(mut self, index: usize) -> Self {
        self.current = index;
        self
    }
}

impl RenderOnce for Steps {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let current = self.current;
        h_flex()
            .gap(Space::S5)
            .children(self.labels.into_iter().enumerate().map(|(index, label)| {
                let place = index.cmp(&current);
                let (tone, state) = match place {
                    Ordering::Less => (Tone::Primary, PaintState::Rest),
                    Ordering::Equal => (Tone::Primary, PaintState::Selected),
                    Ordering::Greater => (Tone::Neutral, PaintState::Rest),
                };
                let surface = Surface::new(SurfaceRole::STEP).tone(tone);
                let label = Label::new(TextStyle::Body, label);
                let label = match place {
                    Ordering::Less => label,
                    Ordering::Equal => label.strong(),
                    Ordering::Greater => label.muted(),
                };
                h_flex()
                    .gap(Space::S5)
                    .when(index > 0, |step| {
                        step.child(
                            div()
                                .w(rems_from_px(LINE))
                                .h(px(LINE_WIDTH))
                                .surface(surface, cx),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(rems_from_px(DOT))
                            .surface_at(surface, state, cx)
                            .map(|dot| match place {
                                Ordering::Less => {
                                    dot.child(Icon::new(IconName::Check).with_size(px(CHECK)))
                                }
                                _ => dot.child(
                                    Label::new(TextStyle::Caption, (index + 1).to_string())
                                        .strong(),
                                ),
                            }),
                    )
                    .child(label)
            }))
    }
}
