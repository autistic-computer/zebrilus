use ala::{
    AnyElement, App, Div, ElementId, SharedString, Stateful, Styled, div, prelude::*, px, relative,
};

use crate::{
    ActiveTheme as _, CHROME_GAP, Card, CardLook, Colorize as _, Emphasis, Icon, IconName, Label,
    PaintExt as _, PaintState, Panel, PanelLook, Sizable as _, StyledExt as _, TextStyle,
    blocks::touch::CoarsePointer,
    components::button::{Button, ButtonLook},
    components::check::Check,
    components::icon::cap_center_drop,
    h_flex, v_flex,
};

use super::activity::{activity_color, transcript_text};

pub const CHAT_CONTENT_MAX_WIDTH: f32 = 680.0;
pub const USER_MESSAGE_GROUP: &str = "chat-user-message";
pub const ASSISTANT_MESSAGE_GROUP: &str = "chat-assistant-message";

#[must_use]
pub fn message_action(button: Button, cx: &App) -> Button {
    button
        .look(ButtonLook::RowIcon)
        .text_color(activity_color(cx))
}

#[must_use]
pub fn shown_on_hover(group: &'static str, cx: &App) -> Div {
    div().flex_none().when(!CoarsePointer::get(cx), |slot| {
        slot.invisible().group_hover(group, Styled::visible)
    })
}

#[derive(IntoElement)]
pub struct UserMessage {
    id: ElementId,
    attachments: Vec<AnyElement>,
    body: Option<AnyElement>,
    action: Option<(AnyElement, bool)>,
}

impl UserMessage {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            attachments: Vec::new(),
            body: None,
            action: None,
        }
    }

    #[must_use]
    pub fn attachment(mut self, attachment: impl IntoElement) -> Self {
        self.attachments.push(attachment.into_any_element());
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }

    #[must_use]
    pub fn action(mut self, action: impl IntoElement, enabled: bool) -> Self {
        self.action = Some((action.into_any_element(), enabled));
        self
    }
}

impl RenderOnce for UserMessage {
    fn render(self, _: &mut ala::Window, cx: &mut App) -> impl IntoElement {
        let bubble = transcript_text(
            Card::new("chat-user-bubble", CardLook::Bubble)
                .debug_selector(|| "chat-user-bubble".to_owned())
                .v_flex()
                .max_w(relative(1.0))
                .gap_2()
                .px_3()
                .py_2(),
            cx,
        )
        .when(!self.attachments.is_empty(), |this| {
            this.child(h_flex().flex_wrap().gap_1().children(self.attachments))
        })
        .children(self.body);
        let entry = v_flex().id(self.id).w_full().items_end();
        let Some((action, enabled)) = self.action else {
            return entry.child(bubble);
        };
        entry.group(USER_MESSAGE_GROUP).child(
            h_flex()
                .w_full()
                .justify_end()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_none()
                        .invisible()
                        .when(enabled, |slot| {
                            slot.group_hover(USER_MESSAGE_GROUP, Styled::visible)
                        })
                        .child(action),
                )
                .child(bubble.min_w_0()),
        )
    }
}

pub fn reply_bubble(
    id: impl Into<ElementId>,
    note: Option<SharedString>,
    body: impl IntoElement,
    cx: &App,
) -> Panel {
    transcript_text(
        Panel::new(id, PanelLook::Inline)
            .debug_selector(|| "chat-reply".to_owned())
            .v_flex()
            .max_w(relative(0.85))
            .gap_1()
            .px_3()
            .py_2(),
        cx,
    )
    .when_some(note, |reply, note| {
        reply.child(Label::new(TextStyle::Micro, note).text_color(cx.theme().foreground.muted()))
    })
    .child(body)
}

pub fn notice_row(id: impl Into<ElementId>, body: impl IntoElement, cx: &App) -> Stateful<Div> {
    let size = TextStyle::ReadBody.design_px();
    transcript_text(
        h_flex()
            .id(id)
            .debug_selector(|| "chat-notice".to_owned())
            .w_full()
            .items_start()
            .gap_2(),
        cx,
    )
    .text_color(activity_color(cx))
    .child(
        div()
            .flex_none()
            .h(px(TextStyle::ReadBody.line_height_px()))
            .flex()
            .items_center()
            .relative()
            .top(cap_center_drop(px(size), px(size), cx))
            .child(Icon::new(IconName::Info).size(px(size))),
    )
    .child(div().flex_1().min_w_0().child(body))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanState {
    Pending,
    Current,
    Done,
}

pub fn plan_list(items: &[(PlanState, SharedString)], cx: &App) -> Div {
    const FONT: f32 = 12.0;
    const MARKER: f32 = 12.0;
    const LINE: f32 = 19.0;
    let drop = cap_center_drop(px(FONT), px(MARKER), cx);
    let muted = cx.theme().foreground.muted();
    let ring = cx.theme().paint(cx.theme().detail, Emphasis::Outline);
    let dot = cx.theme().paint(cx.theme().detail, Emphasis::Solid).fill;
    v_flex()
        .w_full()
        .children(items.iter().map(|(state, text)| {
            let marker = match state {
                PlanState::Pending | PlanState::Done => Check::checkbox(*state == PlanState::Done)
                    .with_size(px(MARKER))
                    .into_any_element(),
                PlanState::Current => h_flex()
                    .size(px(MARKER))
                    .flex_none()
                    .justify_center()
                    .rounded(cx.theme().radius / 2.0)
                    .painted(ring, PaintState::Rest)
                    .child(div().size(px(6.0)).rounded(px(1.5)).bg(dot))
                    .into_any_element(),
            };
            h_flex()
                .w_full()
                .items_start()
                .gap(px(CHROME_GAP))
                .child(
                    div()
                        .h(px(LINE))
                        .flex_none()
                        .flex()
                        .items_center()
                        .relative()
                        .top(drop)
                        .child(marker),
                )
                .child(
                    Label::new(TextStyle::Small, text.clone())
                        .line_height(px(LINE))
                        .flex_1()
                        .min_w_0()
                        .when(*state == PlanState::Done, |text| {
                            text.text_color(muted).line_through()
                        })
                        .when(*state == PlanState::Current, Label::strong),
                )
        }))
}
