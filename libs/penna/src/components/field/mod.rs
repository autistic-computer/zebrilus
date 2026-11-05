use ala::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Label, Sizable as _, Size, Space,
    StyledExt as _, TextStyle, components::icon::cap_center_drop, h_flex, stack,
};

const NOTE: TextStyle = TextStyle::Small;
const MARK: Size = Size::Small;

#[derive(Default, IntoElement)]
pub struct Field {
    label: Option<SharedString>,
    help: Option<SharedString>,
    error: Option<SharedString>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Field {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn help(mut self, help: impl Into<SharedString>) -> Self {
        self.help = Some(help.into());
        self
    }

    #[must_use]
    pub fn error(mut self, error: impl Into<SharedString>) -> Self {
        self.error = Some(error.into());
        self
    }
}

impl ParentElement for Field {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Field {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn error(message: SharedString, cx: &App) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap(MARK.icon_gap())
        .text_color(cx.theme().danger)
        .child(
            h_flex().flex_none().h(NOTE.line_height()).child(
                Icon::new(IconName::CircleX)
                    .with_size(MARK)
                    .relative()
                    .top(cap_center_drop(
                        px(NOTE.design_px()),
                        px(MARK.icon_px()),
                        cx,
                    )),
            ),
        )
        .child(Label::new(NOTE, message).flex_1().min_w_0())
}

impl RenderOnce for Field {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().foreground.muted();
        let note = match (self.error, self.help) {
            (Some(message), _) => Some(error(message, cx).into_any_element()),
            (None, Some(help)) => Some(Label::new(NOTE, help).text_color(muted).into_any_element()),
            (None, None) => None,
        };
        stack(Space::GAP_TIGHT)
            .w_full()
            .items_start()
            .refine_style(&self.style)
            .when_some(self.label, |field, label| {
                field.child(Label::new(TextStyle::Body, label).strong())
            })
            .children(self.children)
            .children(note)
    }
}
