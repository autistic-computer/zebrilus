use ala::{
    App, Div, IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString, Styled as _,
    Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Ground, Icon, IconName, Sizable as _, Size, Space,
    StyledExt as _, TextStyle, components::spinner::Spinner, foundation::muted_on, v_flex,
};

pub const ACTION_BAR_BOTTOM: f32 = 20.0;
pub const ACTION_BAR_KEYBOARD: f32 = 8.0;
pub const EMPTY_GLYPH: f32 = 32.0;

#[must_use]
pub fn action_bar(bottom: Pixels) -> Div {
    v_flex()
        .flex_none()
        .w_full()
        .gap(Space::GAP_RELATED)
        .pt(Space::GAP_RELATED)
        .px(Space::S16)
        .pb(bottom)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EmptyMark {
    Icon(IconName),
    Spinner,
}

#[derive(IntoElement)]
pub struct PhoneEmpty {
    mark: EmptyMark,
    title: SharedString,
    line: Option<SharedString>,
    ground: Option<Ground>,
}

impl PhoneEmpty {
    #[must_use]
    pub fn new(icon: IconName, title: impl Into<SharedString>) -> Self {
        Self {
            mark: EmptyMark::Icon(icon),
            title: title.into(),
            line: None,
            ground: None,
        }
    }

    #[must_use]
    pub fn spinner(title: impl Into<SharedString>) -> Self {
        Self {
            mark: EmptyMark::Spinner,
            ..Self::new(IconName::Loader, title)
        }
    }

    #[must_use]
    pub fn line(mut self, line: impl Into<SharedString>) -> Self {
        self.line = Some(line.into());
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }
}

impl RenderOnce for PhoneEmpty {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let muted = self
            .ground
            .map_or_else(|| theme.foreground.muted(), |ground| muted_on(ground, cx));
        let glyph = px(EMPTY_GLYPH);
        let mark = match self.mark {
            EmptyMark::Icon(icon) => Icon::new(icon)
                .size(glyph)
                .text_color(muted)
                .into_any_element(),
            EmptyMark::Spinner => Spinner::new()
                .with_size(Size::Size(glyph))
                .color(muted)
                .into_any_element(),
        };
        v_flex()
            .w_full()
            .items_center()
            .gap(Space::GAP_TIGHT)
            .px(Space::S32)
            .text_center()
            .font_family(theme.font_family.clone())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .size(glyph)
                    .mb(Space::S8)
                    .child(mark),
            )
            .child(
                div()
                    .type_style(TextStyle::Title, cx)
                    .text_color(theme.foreground)
                    .child(self.title),
            )
            .when_some(self.line, |empty, line| {
                empty.child(
                    div()
                        .type_style(TextStyle::TouchSmall, cx)
                        .text_color(muted)
                        .child(line),
                )
            })
    }
}
