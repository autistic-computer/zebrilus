use ala::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Label, Sizable, Size, Space, StyledExt as _,
    TextStyle, components::icon::cap_center_drop, h_flex, stack,
};

#[derive(Clone, Copy)]
struct Notes {
    label: TextStyle,
    note: TextStyle,
    mark: Size,
}

impl Notes {
    fn of(size: Size) -> Self {
        if size == Size::TouchLarge {
            Self {
                label: TextStyle::TouchSmall,
                note: TextStyle::TouchSmall,
                mark: Size::Touch,
            }
        } else {
            Self {
                label: TextStyle::Body,
                note: TextStyle::Small,
                mark: Size::Small,
            }
        }
    }
}

#[derive(Default, IntoElement)]
pub struct Field {
    size: Size,
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

impl Sizable for Field {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
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

fn error(message: SharedString, notes: Notes, cx: &App) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap(Space::S4)
        .text_color(cx.theme().danger)
        .child(
            h_flex().flex_none().h(notes.note.line_height()).child(
                Icon::new(IconName::CircleX)
                    .with_size(notes.mark)
                    .relative()
                    .top(cap_center_drop(
                        px(notes.note.design_px()),
                        px(notes.mark.icon_px()),
                        cx,
                    )),
            ),
        )
        .child(Label::new(notes.note, message).flex_1().min_w_0())
}

impl RenderOnce for Field {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().foreground.muted();
        let notes = Notes::of(self.size);
        let note = match (self.error, self.help) {
            (Some(message), _) => Some(error(message, notes, cx).into_any_element()),
            (None, Some(help)) => Some(
                Label::new(notes.note, help)
                    .text_color(muted)
                    .into_any_element(),
            ),
            (None, None) => None,
        };
        stack(Space::GAP_TIGHT)
            .w_full()
            .map(|field| {
                if self.size == Size::TouchLarge {
                    field.items_stretch()
                } else {
                    field.items_start()
                }
            })
            .refine_style(&self.style)
            .when_some(self.label, |field, label| {
                field.child(Label::new(notes.label, label).strong())
            })
            .children(self.children)
            .children(note)
    }
}
