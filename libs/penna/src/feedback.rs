use crate::{
    ActiveTheme as _, Ground, Icon, IconName, StyledExt as _,
    dialog::{Dialog, DialogButtonProps},
    h_flex,
    overlay::{dialog_description, dialog_footer, dialog_title},
    roles, v_flex,
};
use ala::{
    App, Div, ParentElement as _, SharedString, Styled as _, div, prelude::FluentBuilder as _, px,
};

const ALERT_LINE: f32 = 16.0;
const ALERT_TITLE_SIZE: f32 = 13.0;
const ALERT_MESSAGE_SIZE: f32 = 12.0;

pub fn inline_alert(
    title: Option<SharedString>,
    message: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let titled = title.is_some();
    let first_line = if titled {
        ALERT_TITLE_SIZE
    } else {
        ALERT_MESSAGE_SIZE
    };
    v_flex()
        .w_full()
        .p(px(12.0))
        .gap(px(12.0))
        .surface(roles::INLINE_PANEL, cx)
        .child(
            h_flex()
                .w_full()
                .items_start()
                .gap_2()
                .child(
                    h_flex().flex_none().h(px(ALERT_LINE)).child(
                        Icon::new(IconName::TriangleAlert)
                            .medium()
                            .text_color(cx.theme().danger)
                            .relative()
                            .top(crate::icon::cap_center_drop(px(first_line), px(14.0), cx)),
                    ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .when_some(title, |column, title| {
                            column.child(dialog_title().line_height(px(ALERT_LINE)).child(title))
                        })
                        .child(
                            dialog_description(cx)
                                .line_height(px(ALERT_LINE))
                                .when(!titled, |message| message.text_color(cx.theme().foreground))
                                .child(message.into()),
                        ),
                ),
        )
}

pub fn alert_actions(cx: &App) -> Div {
    dialog_footer(cx).flex_wrap()
}

pub fn alert_ground(cx: &App) -> Ground {
    cx.theme().ground_of(roles::INLINE_PANEL)
}

pub fn prompt_dialog(
    dialog: Dialog,
    title: impl Into<SharedString>,
    buttons: DialogButtonProps,
) -> Dialog {
    dialog
        .title(title.into())
        .close_button(false)
        .overlay_closable(false)
        .button_props(buttons)
}

pub fn prompt_body() -> Div {
    v_flex().gap_2()
}

pub fn prompt_question(question: &str, cx: &App) -> Div {
    dialog_description(cx)
        .v_flex()
        .gap_1()
        .children(question.lines().map(|line| div().child(line.to_owned())))
}
