use ala::{
    AnyElement, App, IntoElement, ParentElement as _, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Ground, Icon, IconName, Label, Sizable as _, Space, StyledExt as _, Surface,
    SurfaceRole, TextStyle, Tone,
    components::{chip, icon::cap_center_drop},
    h_flex, stack, v_flex,
};

const ICON: f32 = 16.0;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Lead {
    Icon(IconName),
    Dot(Tone),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Layout {
    #[default]
    Inline,
    Stacked,
}

#[derive(IntoElement)]
pub struct Notice {
    tone: Tone,
    title: SharedString,
    detail: Option<SharedString>,
    lead: Lead,
    layout: Layout,
    action: Option<AnyElement>,
    refinement: StyleRefinement,
}

impl Notice {
    #[must_use]
    pub fn new(tone: Tone, title: impl Into<SharedString>) -> Self {
        Self {
            tone,
            title: title.into(),
            detail: None,
            lead: Lead::Icon(icon_for(tone)),
            layout: Layout::Inline,
            action: None,
            refinement: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        let detail = detail.into();
        self.detail = (!detail.is_empty()).then_some(detail);
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.lead = Lead::Icon(icon);
        self
    }

    #[must_use]
    pub fn dot(mut self, tone: Tone) -> Self {
        self.lead = Lead::Dot(tone);
        self
    }

    #[must_use]
    pub fn stacked(mut self) -> Self {
        self.layout = Layout::Stacked;
        self
    }

    #[must_use]
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    #[must_use]
    pub fn ground(tone: Tone, cx: &App) -> Ground {
        cx.theme().ground_of(surface(tone))
    }
}

const fn icon_for(tone: Tone) -> IconName {
    match tone {
        Tone::Danger => IconName::CircleX,
        Tone::Warning => IconName::TriangleAlert,
        Tone::Success => IconName::CircleCheck,
        Tone::Neutral | Tone::Primary | Tone::Accent => IconName::Info,
    }
}

const fn surface(tone: Tone) -> Surface {
    Surface::new(SurfaceRole::NOTICE).tone(tone)
}

impl Styled for Notice {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Notice {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ground = Self::ground(self.tone, cx);
        let first_line = match self.layout {
            Layout::Inline => TextStyle::Body,
            Layout::Stacked => TextStyle::Lead,
        };
        let lead = match self.lead {
            Lead::Icon(icon) => h_flex()
                .flex_none()
                .h(first_line.line_height())
                .child(
                    Icon::new(icon)
                        .with_size(px(ICON))
                        .text_color(cx.theme().tone(self.tone))
                        .relative()
                        .top(cap_center_drop(px(first_line.design_px()), px(ICON), cx)),
                )
                .into_any_element(),
            Lead::Dot(tone) => chip::dot(tone, ground, cx).into_any_element(),
        };
        let title = Label::new(first_line, self.title).font_semibold();
        let detail = self
            .detail
            .map(|detail| Label::new(TextStyle::Small, detail).muted().on(ground));
        let notice = match self.layout {
            Layout::Inline => h_flex()
                .items_start()
                .gap(Space::GAP_RELATED)
                .child(lead)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(Space::GAP_TIGHT)
                        .child(title)
                        .children(detail),
                )
                .children(self.action),
            Layout::Stacked => stack(Space::GAP_RELATED)
                .child(
                    stack(Space::GAP_TIGHT)
                        .child(h_flex().gap(Space::GAP_RELATED).child(lead).child(title))
                        .children(detail),
                )
                .when_some(self.action, |notice, action| {
                    notice.child(h_flex().child(action))
                }),
        };
        notice
            .w_full()
            .p(Space::INSET_CARD)
            .surface(surface(self.tone), cx)
            .type_style(TextStyle::Body, cx)
            .refine_style(&self.refinement)
    }
}
