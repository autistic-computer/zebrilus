use zz_gpui::{
    App, Div, ElementId, GlassMaterial, ParentElement as _, Pixels, ScrollHandle, SharedString,
    Styled as _, div, prelude::*, px, transparent_black,
};

use crate::{
    ActiveTheme as _, Colorize as _, Edge, PANE_HEADER_CONTROL_HEIGHT, StyledExt as _,
    button::{Button, ButtonVariants as _},
    rems_from_px, roles,
};

pub const LARGE_TITLE_HEIGHT: f32 = 52.0;
pub const TITLE_BAR_HEIGHT: f32 = 44.0;

const FOLD_START: f32 = 4.0;
const FROST_SPAN: f32 = 24.0;
const TITLE_SPAN: f32 = 14.0;
const FROST_BLUR: f32 = 20.0;
const FROST_TINT: f32 = 0.72;
const FROST_SATURATION: f32 = 1.8;

/// How far a page has scrolled down, in points.
pub fn scrolled(scroll: &ScrollHandle) -> f32 {
    (-f32::from(scroll.offset().y)).max(0.0)
}

/// A page's title as the first row it scrolls, large and bold, for a
/// [`folding_title_bar`] to take over once it scrolls away.
pub fn large_title(title: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .debug_selector(|| "large-title".to_owned())
        .flex()
        .flex_none()
        .items_end()
        .h(px(LARGE_TITLE_HEIGHT))
        .px(px(16.0))
        .pb(px(4.0))
        .text_size(px(34.0))
        .line_height(px(41.0))
        .font_bold()
        .text_color(cx.theme().foreground)
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(title.into()),
        )
}

/// The bar over a page that opens with a [`large_title`], `top_inset` under
/// the window's top. Clear while the large title shows, it frosts over
/// what scrolls under it and takes the title, small and centered, once the
/// page has `scrolled` the large one away.
pub fn folding_title_bar(
    title: impl Into<SharedString>,
    scrolled: f32,
    top_inset: Pixels,
    cx: &App,
) -> Div {
    let frost = ((scrolled - FOLD_START) / FROST_SPAN).clamp(0.0, 1.0);
    let shown = ((scrolled - FOLD_START - FROST_SPAN) / TITLE_SPAN).clamp(0.0, 1.0);
    let theme = cx.theme();
    let material = GlassMaterial {
        blur: px(FROST_BLUR),
        bezel: px(0.0),
        tint: theme.background.alpha(FROST_TINT),
        saturation: FROST_SATURATION,
        merge: px(0.0),
        opacity: frost,
        ..GlassMaterial::regular().vanished()
    };
    div()
        .debug_selector(|| "folding-title-bar".to_owned())
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(top_inset + px(TITLE_BAR_HEIGHT))
        .when(frost > 0.0, |bar| {
            bar.child(
                div()
                    .absolute()
                    .inset_0()
                    .glass(material)
                    .bg(transparent_black()),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .opacity(frost)
                    .rule_on(roles::PHONE_BAR, Edge::Bottom, cx),
            )
        })
        .when(shown > 0.0, |bar| {
            bar.child(
                div()
                    .absolute()
                    .left(px(56.0))
                    .right(px(56.0))
                    .bottom_0()
                    .h(px(TITLE_BAR_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(shown)
                    .text_size(px(17.0))
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(title.into()),
                    ),
            )
        })
}

/// A pane header button showing how many tabs a page holds, the count in a
/// rounded square; it opens the page's tab drawer.
#[allow(
    clippy::disallowed_methods,
    reason = "the box is a glyph in the button's text color, not a surface edge"
)]
pub fn compact_tabs_button(id: impl Into<ElementId>, count: usize, cx: &App) -> Button {
    let color = cx.theme().foreground.faint();
    Button::new(id)
        .ghost()
        .compact()
        .surface_role(roles::BROWSER_BUTTON)
        .size(PANE_HEADER_CONTROL_HEIGHT)
        .text_color(color)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(rems_from_px(15.0))
                .rounded(rems_from_px(4.0))
                .border(rems_from_px(1.5))
                .border_color(color)
                .text_size(rems_from_px(9.0))
                .line_height(rems_from_px(10.0))
                .font_semibold()
                .child(SharedString::from(count.min(99).to_string())),
        )
}
