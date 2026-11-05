mod activity;
mod attachment;
mod card;
mod composer;
mod message;
mod suggestions;
mod tasks;

use std::ops::Range;

use ala::{
    App, Div, EntityId, HighlightStyle, SharedString, StyledText, Transformation, ease_in_out,
    percentage, prelude::*, px,
};
use web_time::Duration;

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Label, StyledExt as _, TextStyle,
    foundation::pulse::pulse_phase, v_flex,
};

pub use activity::{
    ACTIVITY_DISCLOSURE_SIZE, ACTIVITY_ROW_FONT_SIZE, ACTIVITY_ROW_HEIGHT,
    ACTIVITY_ROW_LINE_HEIGHT, ActivityCount, ActivityEnd, ActivityMark, activity_color,
    activity_glyph, activity_row, disclosure_icon, single_line, transcript_ground, transcript_text,
};
pub use attachment::{
    COMPOSER_ATTACHMENT, TRANSCRIPT_ATTACHMENT, attachment_tile, composer_attachment,
};
pub use card::{
    CARD_GAP, CARD_GLYPH, CARD_GUTTER, CARD_TITLE_LINE, CardTile, card_button, card_chip,
    card_close, card_divider, card_footer, card_glyph, card_header, card_surface, card_tiles,
    card_title, card_trailing, permission_card, permission_option,
};
pub use composer::{
    COMPOSER_BUTTON, COMPOSER_FOOTER_HEIGHT, COMPOSER_HEIGHT, COMPOSER_INSET, COMPOSER_MAX_WIDTH,
    COMPOSER_OUTER_PADDING, COMPOSER_SECTION_GAP, ChatComposer, Composer, ComposerAction,
    chrome_button, composer_action_button, composer_add_button, composer_ground, context_meter,
    queue_chip,
};
pub use message::{
    ASSISTANT_MESSAGE_GROUP, CHAT_CONTENT_MAX_WIDTH, PlanState, USER_MESSAGE_GROUP, UserMessage,
    message_action, notice_row, plan_list, reply_bubble, shown_on_hover,
};
pub use suggestions::{suggestion_list, suggestion_list_height, suggestion_row};
pub use tasks::{TaskItem, TaskTrayAction, TrayPanel, TrayPlan, task_tray};

const SHIMMER_MS: u64 = 1600;

pub fn spinner_phase(view: EntityId, cx: &mut App) -> f32 {
    ease_in_out(pulse_phase(Duration::from_millis(800), view, cx))
}

#[must_use]
pub fn spinner(phase: f32) -> Icon {
    Icon::new(IconName::Loader).transform(Transformation::rotate(percentage(phase)))
}

#[must_use]
pub fn shimmer(label: &str, now: u64, cx: &App) -> StyledText {
    let text = format!("{label}…");
    let count = text.chars().count();
    let span = u64::try_from(count + 8).unwrap_or(u64::MAX);
    let head = usize::try_from((now % SHIMMER_MS) * span / SHIMMER_MS).unwrap_or_default();
    let theme = cx.theme();
    let mut highlights: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    for (index, (offset, character)) in text.char_indices().enumerate() {
        let distance = (index + 4).abs_diff(head);
        let color = match distance {
            0 | 1 => theme.foreground,
            2 | 3 => theme.foreground.muted(),
            _ => theme.foreground.faint(),
        };
        let range = offset..offset + character.len_utf8();
        match highlights.last_mut() {
            Some((last, style)) if style.color == Some(color) && last.end == range.start => {
                last.end = range.end;
            }
            _ => highlights.push((
                range,
                HighlightStyle {
                    color: Some(color),
                    ..HighlightStyle::default()
                },
            )),
        }
    }
    StyledText::new(text).with_highlights(highlights)
}

pub fn welcome_state(
    icon: Icon,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    cx: &App,
) -> Div {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .px_3()
        .pb(px(32.0))
        .text_center()
        .child(icon.mb(px(24.0)).text_color(cx.theme().foreground.muted()))
        .child(Label::new(TextStyle::Title, title).text_color(cx.theme().foreground))
        .child(
            Label::new(TextStyle::Body, detail)
                .mt(px(10.0))
                .text_color(cx.theme().foreground.muted()),
        )
}

pub fn empty_state(
    message: impl Into<SharedString>,
    busy: bool,
    view: EntityId,
    cx: &mut App,
) -> Div {
    let phase = if busy { spinner_phase(view, cx) } else { 0.0 };
    v_flex()
        .w_full()
        .py(px(48.0))
        .items_center()
        .gap_2()
        .type_style(TextStyle::Small, cx)
        .text_color(cx.theme().foreground.muted())
        .when(busy, |column| {
            column.child(
                spinner(phase)
                    .medium()
                    .text_color(cx.theme().foreground.muted()),
            )
        })
        .child(message.into())
}
