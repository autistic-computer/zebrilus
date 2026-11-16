mod activity;
mod ask;
mod attachment;
mod card;
mod composer;
mod figure;
mod message;
mod phone;
mod resolve;
mod suggestions;
mod tasks;

use ala::{App, Div, EntityId, SharedString, Transformation, percentage, prelude::*, px};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Label, StyledExt as _, TextStyle,
    foundation::motion::SPIN, foundation::pulse::pulse_phase, v_flex,
};

pub use activity::{
    ACTIVITY_DISCLOSURE_SIZE, ACTIVITY_ROW_FONT_SIZE, ACTIVITY_ROW_HEIGHT,
    ACTIVITY_ROW_LINE_HEIGHT, ActivityCount, ActivityEnd, ActivityMark, TOUCH_ACTIVITY_GLYPH,
    activity_color, activity_glyph, activity_row, disclosure_icon, single_line,
    touch_transcript_text, transcript_ground, transcript_text,
};
pub use ask::{
    ASK_GAP, ASK_GUTTER, ASK_RADIUS, AskDetail, AskHint, AskOption, ask_body, ask_count,
    ask_divider, ask_header, ask_hints, ask_inset, ask_key, ask_leading, ask_note, ask_options,
    ask_picked_key, ask_question, ask_send, ask_sheet, ask_skip, ask_step,
};
pub use attachment::{
    COMPOSER_ATTACHMENT, TRANSCRIPT_ATTACHMENT, attachment_tile, composer_attachment,
};
pub use card::{
    CARD_GAP, CARD_GLYPH, CARD_GUTTER, CARD_TITLE_LINE, card_button, card_chip, card_glyph,
    card_header, card_title,
};
pub use composer::{
    COMPOSER_BUTTON, COMPOSER_FOOTER_HEIGHT, COMPOSER_HEIGHT, COMPOSER_INSET, COMPOSER_MAX_WIDTH,
    COMPOSER_OUTER_PADDING, COMPOSER_SECTION_GAP, ChatComposer, Composer, ComposerAction,
    chrome_button, composer_action_button, composer_add_button, composer_ground,
    composer_icon_button, context_meter, context_ring, queue_chip,
};
pub use figure::{DiagramPaint, diagram_paint};
pub use message::{
    ASSISTANT_MESSAGE_GROUP, CHAT_CONTENT_MAX_WIDTH, PlanState, USER_MESSAGE_GROUP, UserMessage,
    message_action, notice_row, plan_list, reply_bubble, shown_on_hover,
};
pub(crate) use phone::phone_button;
pub use phone::{
    AskCard, PHONE_COMPOSER_BUTTON, PHONE_COMPOSER_HEIGHT, PHONE_COMPOSER_INSET,
    PHONE_COMPOSER_ROWS, PHONE_QUEUED_HEIGHT, PHONE_THUMBNAIL, PhoneComposer, phone_action_button,
    phone_close_button, phone_composer_corner, phone_jump_button, phone_plan_tray,
    phone_plus_button, phone_queued_row, phone_thumbnail,
};
pub use suggestions::{suggestion_list, suggestion_list_height, suggestion_row};
pub use tasks::{TrayPlan, plan_tray};

pub fn spinner_phase(view: EntityId, cx: &mut App) -> f32 {
    pulse_phase(SPIN, view, cx)
}

#[must_use]
pub fn spinner(phase: f32) -> Icon {
    Icon::new(IconName::Loader).transform(Transformation::rotate(percentage(phase)))
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
