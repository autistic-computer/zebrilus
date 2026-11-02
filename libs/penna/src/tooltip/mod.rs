use ala::{
    Action, AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    SharedString, StyleRefinement, Styled, Window, div, prelude::FluentBuilder,
};

use crate::Colorize as _;
use crate::{ActiveTheme as _, StyledExt as _, SurfaceRole, h_flex, kbd::Kbd};

pub struct Tooltip {
    style: StyleRefinement,
    text: SharedString,
    action: Option<(Box<dyn Action>, Option<SharedString>)>,
}

impl Tooltip {
    #[must_use]
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            style: StyleRefinement::default(),
            text: text.into(),
            action: None,
        }
    }

    #[must_use]
    pub fn action(mut self, action: &dyn Action, context: Option<&str>) -> Self {
        self.action = Some((action.boxed_clone(), context.map(SharedString::new)));
        self
    }

    #[must_use]
    pub fn build(self, _window: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl FluentBuilder for Tooltip {}

impl Styled for Tooltip {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let key_binding = self.action.as_ref().and_then(|(action, context)| {
            Kbd::binding_for_action(action.as_ref(), context.as_deref(), window)
        });

        div().child(
            h_flex()
                .m_3()
                .px_2()
                .py_0p5()
                .gap_3()
                .justify_between()
                .font_family(cx.theme().font_family.clone())
                .text_xs()
                .surface(SurfaceRole::TOOLTIP, cx)
                .refine_style(&self.style)
                .child(div().child(self.text.clone()))
                .when_some(key_binding, |this, kbd| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(cx.theme().foreground.muted())
                            .child(kbd.text()),
                    )
                }),
        )
    }
}
