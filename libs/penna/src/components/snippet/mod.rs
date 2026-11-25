use ala::{
    App, ClipboardItem, ElementId, IntoElement, ParentElement as _, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _,
};
use web_time::{Duration, Instant};

use crate::{
    ActiveTheme as _, Colorize as _, IconName, Sizable as _, Space, StyledExt as _, Surface,
    TextStyle,
    components::button::{Button, ButtonVariants as _},
    foundation::roles,
    h_flex,
};

const COPIED_HOLD: Duration = Duration::from_millis(1400);

#[derive(IntoElement)]
pub struct Snippet {
    id: ElementId,
    code: SharedString,
    prefix: Option<SharedString>,
    copyable: bool,
    style: StyleRefinement,
}

impl Snippet {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, code: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            code: code.into(),
            prefix: Some("$".into()),
            copyable: true,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn prefix(mut self, prefix: Option<impl Into<SharedString>>) -> Self {
        self.prefix = prefix.map(Into::into);
        self
    }

    #[must_use]
    pub const fn copyable(mut self, copyable: bool) -> Self {
        self.copyable = copyable;
        self
    }
}

impl Styled for Snippet {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Snippet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let copied_at =
            window.use_keyed_state((self.id.clone(), "copied"), cx, |_, _| None::<Instant>);
        let now = cx.background_executor().now();
        let copied = copied_at
            .read(cx)
            .is_some_and(|at| now.saturating_duration_since(at) < COPIED_HOLD);
        let theme = cx.theme();
        let surface = Surface::new(roles::CARD_CODE);
        let ground = theme.ground_of(surface);
        let code_for_copy = self.code.clone();

        h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(Space::S8)
            .pl(Space::S12)
            .pr(Space::S6)
            .py(Space::S6)
            .surface(surface, cx)
            .refine_style(&self.style)
            .when_some(self.prefix, |row, prefix| {
                row.child(
                    div()
                        .flex_none()
                        .font_family(theme.mono_font_family.clone())
                        .type_style(TextStyle::Small, cx)
                        .text_color(theme.foreground.faint())
                        .child(prefix),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(theme.mono_font_family.clone())
                    .type_style(TextStyle::Small, cx)
                    .text_color(theme.foreground)
                    .child(self.code),
            )
            .when(self.copyable, |row| {
                let copied_at = copied_at.clone();
                let icon = if copied {
                    IconName::Check
                } else {
                    IconName::Copy
                };
                row.child(
                    Button::new((self.id.clone(), "copy"))
                        .ghost()
                        .small()
                        .icon(icon)
                        .on(ground)
                        .on_click(move |_, window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                code_for_copy.to_string(),
                            ));
                            let now = cx.background_executor().now();
                            copied_at.update(cx, |slot, cx| {
                                *slot = Some(now);
                                cx.notify();
                            });
                            window.request_animation_frame();
                        }),
                )
            })
    }
}
