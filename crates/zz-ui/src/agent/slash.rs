use ala::{App, Div, ElementId, IntoElement, Pixels, SharedString, Stateful, div, prelude::*, px};

use crate::{
    ActiveTheme as _, StyledExt as _, SurfaceRole, h_flex, muted_on, roles, row_state, row_surface,
    v_flex,
};

const ROW_HEIGHT: f32 = 40.0;
const MAX_VISIBLE_ROWS: u8 = 6;
const LIST_PADDING: f32 = 4.0;

pub fn suggestion_list_height(count: usize) -> Pixels {
    px(ROW_HEIGHT
        * f32::from(
            u8::try_from(count)
                .unwrap_or(MAX_VISIBLE_ROWS)
                .min(MAX_VISIBLE_ROWS),
        ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashKind {
    Command,
    Skill,
    Terminal,
}

impl SlashKind {
    #[must_use]
    pub const fn of(terminal: bool, skill: bool) -> Self {
        if terminal {
            Self::Terminal
        } else if skill {
            Self::Skill
        } else {
            Self::Command
        }
    }

    const fn badge(self) -> Option<&'static str> {
        match self {
            Self::Command => None,
            Self::Skill => Some("skill"),
            Self::Terminal => Some("opens terminal"),
        }
    }
}

pub fn suggestion_row(
    id: impl Into<ElementId>,
    label: &str,
    description: Option<SharedString>,
    kind: SlashKind,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let state = row_state(selected, true);
    let muted = muted_on(&cx.theme().layer(SurfaceRole::MENU_ITEM, state), cx);
    row_surface(
        h_flex()
            .id(id)
            .w_full()
            .h(px(ROW_HEIGHT))
            .items_center()
            .gap_3()
            .px_3()
            .cursor_pointer(),
        SurfaceRole::MENU_ITEM,
        state,
        px(ROW_HEIGHT),
        cx,
    )
    .child(
        v_flex()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(crate::rems_from_px(12.0))
                    .line_height(px(16.0))
                    .font_weight(ala::FontWeight::MEDIUM)
                    .child(label.to_owned()),
            )
            .when_some(description, |column, description| {
                column.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(crate::rems_from_px(11.0))
                        .line_height(px(14.0))
                        .text_color(muted)
                        .child(description),
                )
            }),
    )
    .children(kind.badge().map(|badge| {
        div()
            .flex_none()
            .text_size(crate::rems_from_px(10.0))
            .line_height(px(14.0))
            .text_color(muted)
            .child(badge)
    }))
}

pub fn suggestion_list(rows: impl IntoElement, cx: &App) -> Div {
    v_flex()
        .w_full()
        .surface(roles::INLINE_PANEL, cx)
        .list_inset(cx)
        .py(px(LIST_PADDING))
        .overflow_hidden()
        .child(rows)
}
