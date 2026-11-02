use ala::{
    App, Div, GlassMaterial, ParentElement as _, Pixels, ScrollHandle, SharedString, Styled as _,
    div, prelude::*, px, transparent_black,
};

use crate::{ActiveTheme as _, Edge, StyledExt as _, foundation::RoleRuleExt as _, roles};

pub const COMPACT_TITLE_BAR_HEIGHT: f32 = 44.0;

const FOLD_START: f32 = 4.0;
const FROST_SPAN: f32 = 24.0;
const TITLE_SPAN: f32 = 14.0;
const FROST_BLUR: f32 = 20.0;
const FROST_TINT: f32 = 0.72;
const FROST_SATURATION: f32 = 1.8;

pub fn scrolled(scroll: &ScrollHandle) -> f32 {
    (-f32::from(scroll.offset().y)).max(0.0)
}

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
        .h(top_inset + px(COMPACT_TITLE_BAR_HEIGHT))
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
                    .h(px(COMPACT_TITLE_BAR_HEIGHT))
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
