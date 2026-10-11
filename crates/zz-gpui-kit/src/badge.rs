//! A short label that says what something is or how it is doing: a count, a
//! status, a kind.

use zz_gpui::{
    App, FontWeight, IntoElement, ParentElement as _, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, px,
};

use crate::{
    ActiveTheme as _, Emphasis, Icon, PaintExt as _, PaintState, Sizable, Size, StyledExt as _,
    Tone, h_flex, rems_from_px,
};

#[derive(IntoElement)]
pub struct Badge {
    label: SharedString,
    icon: Option<Icon>,
    tone: Tone,
    emphasis: Emphasis,
    size: Size,
    style: StyleRefinement,
}

impl Badge {
    /// A neutral, soft badge.
    #[must_use]
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            tone: Tone::default(),
            emphasis: Emphasis::default(),
            size: Size::default(),
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    #[must_use]
    pub fn emphasis(mut self, emphasis: Emphasis) -> Self {
        self.emphasis = emphasis;
        self
    }

    /// An icon before the label, in the label's color.
    #[must_use]
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

impl Sizable for Badge {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for Badge {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (height, text, padding) = match self.size {
            Size::XSmall => (16.0, 10.0, 5.0),
            Size::Small => (18.0, 11.0, 6.0),
            Size::Large => (24.0, 13.0, 9.0),
            Size::Medium | Size::Size(_) => (20.0, 12.0, 7.0),
        };
        let paint = cx.theme().paint(self.tone, self.emphasis);
        h_flex()
            .flex_none()
            .h(px(height))
            .gap(px(4.0))
            .px(px(padding))
            .rounded(cx.theme().control_radius())
            .painted(paint, PaintState::Rest)
            .text_size(rems_from_px(text))
            .line_height(px(height - 2.0))
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap()
            .refine_style(&self.style)
            .children(self.icon.map(|icon| icon.size(px(text + 1.0))))
            .child(self.label)
    }
}
