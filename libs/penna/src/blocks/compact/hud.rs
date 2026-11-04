use ala::{App, Div, ParentElement as _, SharedString, Styled as _, div, prelude::*};

use crate::{ActiveTheme as _, Space, StyledExt as _, Surface, SurfaceRole};

pub fn compact_hud(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .debug_selector(|| "compact-hud".to_owned())
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .flex_none()
                .px(Space::S16)
                .py(Space::S8)
                .surface(Surface::new(SurfaceRole::TOAST), cx)
                .font_family(cx.theme().font_family.clone())
                .type_style_strong(crate::TextStyle::TouchBody, cx)
                .child(text.into()),
        )
}
