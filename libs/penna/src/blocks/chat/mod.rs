mod activity;
mod ask;
mod attachment;
mod card;
mod composer;
mod figure;
mod message;
mod suggestions;
mod tasks;

use std::f32::consts::PI;
use std::ops::Range;

use ala::{
    App, Div, EntityId, HighlightStyle, SharedString, StyledText, Transformation, percentage,
    prelude::*, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Label, StyledExt as _, TextStyle,
    foundation::motion::SPIN, foundation::pulse::pulse_phase, v_flex,
};

pub use activity::{
    ACTIVITY_DISCLOSURE_SIZE, ACTIVITY_ROW_FONT_SIZE, ACTIVITY_ROW_HEIGHT,
    ACTIVITY_ROW_LINE_HEIGHT, ActivityCount, ActivityEnd, ActivityMark, activity_color,
    activity_glyph, activity_row, disclosure_icon, single_line, transcript_ground, transcript_text,
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
    chrome_button, composer_action_button, composer_add_button, composer_ground, context_meter,
    queue_chip,
};
pub use figure::{DiagramPaint, diagram_paint};
pub use message::{
    ASSISTANT_MESSAGE_GROUP, CHAT_CONTENT_MAX_WIDTH, PlanState, USER_MESSAGE_GROUP, UserMessage,
    message_action, notice_row, plan_list, reply_bubble, shown_on_hover,
};
pub use suggestions::{suggestion_list, suggestion_list_height, suggestion_row};
pub use tasks::{TrayPlan, plan_tray};

const SWEEP_PASS_MS: u64 = 2000;
const SWEEP_REST_MS: u64 = 600;
const SWEEP_CYCLE_MS: u64 = SWEEP_PASS_MS + SWEEP_REST_MS;
const SWEEP_CREST: f32 = 12.0;
const SWEEP_STEPS: f32 = 32.0;
const BREATH_OPACITY: f32 = 0.5;
const BREATH_SCALE: f32 = 0.9;

pub fn spinner_phase(view: EntityId, cx: &mut App) -> f32 {
    pulse_phase(SPIN, view, cx)
}

#[must_use]
pub fn spinner(phase: f32) -> Icon {
    Icon::new(IconName::Loader).transform(Transformation::rotate(percentage(phase)))
}

fn sweep_at(now: u64) -> f32 {
    (now % SWEEP_CYCLE_MS) as f32 / SWEEP_CYCLE_MS as f32
}

fn crest_weights(count: usize, now: u64) -> impl Iterator<Item = u8> {
    let half = SWEEP_CREST / 2.0;
    let pass = (now % SWEEP_CYCLE_MS) as f32 / SWEEP_PASS_MS as f32;
    let crest = -half + pass * (count as f32 + SWEEP_CREST);
    (0..count).map(move |index| {
        let distance = (index as f32 + 0.5 - crest).abs();
        let weight = if distance < half {
            f32::midpoint(1.0, (PI * distance / half).cos())
        } else {
            0.0
        };
        (weight * SWEEP_STEPS).round() as u8
    })
}

fn breath(now: u64, cx: &App) -> (f32, f32) {
    if cx.reduce_motion() {
        return (1.0, 1.0);
    }
    let lift = f32::midpoint(1.0, (2.0 * PI * sweep_at(now)).cos());
    (
        BREATH_OPACITY + (1.0 - BREATH_OPACITY) * lift,
        BREATH_SCALE + (1.0 - BREATH_SCALE) * lift,
    )
}

#[must_use]
pub fn shimmer(label: &str, now: u64, cx: &App) -> StyledText {
    let text = format!("{label}…");
    let theme = cx.theme();
    let rest = theme.foreground.muted();
    let still = cx.reduce_motion();
    let weights = crest_weights(text.chars().count(), now).map(|step| if still { 0 } else { step });
    let mut highlights: Vec<(Range<usize>, u8)> = Vec::new();
    for ((offset, character), step) in text.char_indices().zip(weights) {
        let range = offset..offset + character.len_utf8();
        match highlights.last_mut() {
            Some((last, seen)) if *seen == step => last.end = range.end,
            _ => highlights.push((range, step)),
        }
    }
    StyledText::new(text).with_highlights(highlights.into_iter().map(|(range, step)| {
        let color = theme
            .foreground
            .mix_oklab(rest, f32::from(step) / SWEEP_STEPS);
        (
            range,
            HighlightStyle {
                color: Some(color),
                ..HighlightStyle::default()
            },
        )
    }))
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

#[cfg(test)]
mod tests {
    use super::crest_weights;

    #[test]
    fn the_crest_sweeps_with_a_raised_cosine_and_rests_between_passes() {
        assert_eq!(
            crest_weights(8, 1000).collect::<Vec<_>>(),
            [12, 20, 27, 31, 31, 27, 20, 12]
        );
        assert_eq!(crest_weights(8, 0).collect::<Vec<_>>(), [0; 8]);
        assert_eq!(crest_weights(8, 2300).collect::<Vec<_>>(), [0; 8]);
        assert_eq!(
            crest_weights(8, 3600).collect::<Vec<_>>(),
            [12, 20, 27, 31, 31, 27, 20, 12]
        );
    }
}
