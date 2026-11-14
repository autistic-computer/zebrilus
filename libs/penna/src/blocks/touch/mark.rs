use ala::{App, Hsla, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div, px};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Sizable as _, Size, Tone,
    components::spinner::Spinner,
};

pub const MARK_DOT: f32 = 8.0;
pub const MARK_SPINNER: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoElement)]
pub enum StatusMark {
    Success,
    Warning,
    Danger,
    Ink,
    Off,
    Spinner,
}

impl StatusMark {
    #[must_use]
    pub fn word_ink(self, muted: Hsla, cx: &App) -> Hsla {
        let theme = cx.theme();
        match self {
            Self::Warning => theme.paint(Tone::Warning, Emphasis::Ghost).text,
            Self::Danger => theme.paint(Tone::Danger, Emphasis::Ghost).text,
            Self::Ink => theme.foreground,
            Self::Success | Self::Off | Self::Spinner => muted,
        }
    }

    #[must_use]
    pub fn dot(self, cx: &App) -> Option<Hsla> {
        let theme = cx.theme();
        match self {
            Self::Success => Some(theme.success),
            Self::Warning => Some(theme.warning),
            Self::Danger => Some(theme.danger),
            Self::Ink => Some(theme.foreground),
            Self::Off => Some(theme.foreground.faint()),
            Self::Spinner => None,
        }
    }
}

impl RenderOnce for StatusMark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        match self.dot(cx) {
            Some(color) => div()
                .flex_none()
                .size(px(MARK_DOT))
                .rounded_full()
                .bg(color)
                .into_any_element(),
            None => div()
                .flex()
                .flex_none()
                .size(px(MARK_SPINNER))
                .child(Spinner::new().with_size(Size::Size(px(MARK_SPINNER))))
                .into_any_element(),
        }
    }
}
