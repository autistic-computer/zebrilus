use ala::{App, Div, Hsla, ParentElement as _, Styled as _, div, prelude::*};

use crate::{ActiveTheme as _, Colorize as _, rems_from_px};

const ACTIVE_DOT: f32 = 12.0;
const DOT_GAP: f32 = 3.0;
const GROUP_GAP: f32 = 6.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageDot {
    pub active: bool,
    pub attention: bool,
}

pub fn page_dots(groups: &[Vec<PageDot>], cx: &App) -> Div {
    page_dots_in(groups, cx.theme().foreground, cx)
}

pub fn page_dots_in(groups: &[Vec<PageDot>], ink: Hsla, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(rems_from_px(GROUP_GAP))
        .children(groups.iter().enumerate().map(|(group, dots)| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(rems_from_px(DOT_GAP))
                .children(dots.iter().enumerate().map(|(index, dot)| {
                    let selector = format!("page-dot-{group}-{index}");
                    let color = if dot.active {
                        ink
                    } else if dot.attention {
                        theme.warning
                    } else {
                        ink.faint()
                    };
                    crate::sidebar::status_dot(color)
                        .debug_selector(move || selector)
                        .when(dot.active, |dot| dot.w(rems_from_px(ACTIVE_DOT)))
                }))
        }))
}

#[cfg(test)]
mod tests {
    use ala::{
        Bounds, Context, IntoElement, Pixels, Render, TestAppContext, VisualTestContext, Window,
    };

    use super::*;

    struct Host {
        groups: Vec<Vec<PageDot>>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(page_dots(&self.groups, cx))
        }
    }

    fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
        cx.debug_bounds(selector).expect(selector)
    }

    #[ala::test]
    fn one_element_per_dot_and_the_active_one_is_wider(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let groups = vec![
            vec![
                PageDot::default(),
                PageDot {
                    active: true,
                    attention: false,
                },
            ],
            vec![
                PageDot {
                    active: false,
                    attention: true,
                },
                PageDot::default(),
                PageDot::default(),
            ],
        ];
        let (_, cx) = cx.add_window_view(|_, _| Host { groups });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let rest = bounds(cx, "page-dot-0-0");
        let active = bounds(cx, "page-dot-0-1");
        assert!(active.size.width > rest.size.width);
        assert_eq!(active.size.height, rest.size.height);
        for selector in ["page-dot-1-0", "page-dot-1-1", "page-dot-1-2"] {
            assert_eq!(bounds(cx, selector).size, rest.size);
        }
        assert!(cx.debug_bounds("page-dot-0-2").is_none());
        assert!(cx.debug_bounds("page-dot-1-3").is_none());
        let inner = bounds(cx, "page-dot-1-1").left() - bounds(cx, "page-dot-1-0").right();
        let between = bounds(cx, "page-dot-1-0").left() - active.right();
        assert!(between > inner);
    }
}
