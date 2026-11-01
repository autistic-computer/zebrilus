use ala::{App, ElementId, IntoElement, Pixels, SharedString, prelude::*, px};
use penna::{Label, Panel, PanelLook, Row, RowLook, TextStyle};

use crate::{ActiveTheme as _, StyledExt as _, row_state, v_flex};

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
}

impl SlashKind {
    #[must_use]
    pub const fn of(skill: bool) -> Self {
        if skill { Self::Skill } else { Self::Command }
    }

    const fn badge(self) -> Option<&'static str> {
        match self {
            Self::Command => None,
            Self::Skill => Some("skill"),
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
) -> Row {
    let state = row_state(selected, true);
    let ground = cx.theme().ground_at(RowLook::Menu, state);
    Row::new(id, RowLook::Menu)
        .state(state)
        .row_height(px(ROW_HEIGHT))
        .h_flex()
        .w_full()
        .h(px(ROW_HEIGHT))
        .gap_3()
        .px_3()
        .cursor_pointer()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    Label::new(TextStyle::Small, label.to_owned())
                        .strong()
                        .truncate()
                        .w_full()
                        .min_w_0(),
                )
                .when_some(description, |column, description| {
                    column.child(
                        Label::new(TextStyle::Caption, description)
                            .muted()
                            .on(ground)
                            .truncate()
                            .w_full()
                            .min_w_0()
                            .line_height(px(14.0)),
                    )
                }),
        )
        .children(kind.badge().map(|badge| {
            Label::new(TextStyle::Micro, badge)
                .muted()
                .on(ground)
                .flex_none()
        }))
}

pub fn suggestion_list(rows: impl IntoElement, cx: &App) -> Panel {
    Panel::new("agent-slash-suggestions", PanelLook::Inline)
        .v_flex()
        .w_full()
        .list_inset(cx)
        .py(px(LIST_PADDING))
        .overflow_hidden()
        .child(rows)
}
