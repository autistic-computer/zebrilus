use ala::{
    App, IntoElement, ParentElement as _, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder as _,
};

use crate::{ActiveTheme as _, Emphasis, Ground, PaintState, StyledExt as _, TextStyle, Tone};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Level {
    #[default]
    Full,
    Muted,
    Faint,
}

#[derive(IntoElement)]
pub struct Label {
    text: SharedString,
    style: TextStyle,
    tone: Option<Tone>,
    level: Level,
    ground: Option<Ground>,
    strong: bool,
    truncate: bool,
    refinement: StyleRefinement,
}

impl Label {
    #[must_use]
    pub fn new(style: TextStyle, text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            style,
            tone: None,
            level: Level::Full,
            ground: None,
            strong: false,
            truncate: false,
            refinement: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = Some(tone);
        self
    }

    #[must_use]
    pub fn muted(mut self) -> Self {
        self.level = Level::Muted;
        self
    }

    #[must_use]
    pub fn faint(mut self) -> Self {
        self.level = Level::Faint;
        self
    }

    #[must_use]
    pub fn strong(mut self) -> Self {
        self.strong = true;
        self
    }

    #[must_use]
    pub fn truncate(mut self) -> Self {
        self.truncate = true;
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.ground = Some(ground);
        self
    }
}

impl Styled for Label {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Label {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let ground = self.ground.unwrap_or_else(|| theme.ground());
        let paint = theme.paint_on(ground, self.tone.unwrap_or_default(), Emphasis::Ghost);
        let color = match (self.level, self.tone) {
            (Level::Full, None) => None,
            (Level::Full, Some(_)) => Some(paint.text_at(PaintState::Rest)),
            (Level::Muted, _) => Some(paint.muted_at(PaintState::Rest)),
            (Level::Faint, _) => Some(paint.faint_at(PaintState::Rest)),
        };
        let style = self.style;
        div()
            .type_style(style, cx)
            .refine_style(&self.refinement)
            .when(self.strong, |this| this.font_weight(style.strong_weight()))
            .when_some(color, Styled::text_color)
            .when(self.truncate, Styled::truncate)
            .child(self.text)
    }
}
