use crate::{
    ActiveTheme as _, Emphasis, Ground, Icon, IconName, StyledExt as _, Tone,
    button::ButtonVariant,
    dialog::{AlertDialog, Dialog, DialogButtonProps},
    h_flex,
    input::{Input, InputContentType, InputState},
    overlay::{dialog_description, dialog_footer, dialog_title},
    rems_from_px, roles, v_flex,
};
use ala::{
    App, Div, Entity, ParentElement as _, SharedString, Styled as _, div,
    prelude::FluentBuilder as _, px,
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

#[cfg(any(target_os = "ios", target_os = "android"))]
const ADD_HOST_DESCRIPTION: &str = "Connects over SSH with this device's built-in key.";
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const ADD_HOST_DESCRIPTION: &str = "Connects over SSH · your ~/.ssh/config still applies.";

pub fn browser_clear_site_data_alert(alert: AlertDialog, cx: &App) -> AlertDialog {
    alert
        .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
        .title("Clear site data?")
        .description(
            "This clears cookies and persistent storage owned by the current site. You may be signed out. This cannot be undone.",
        )
        .button_props(
            DialogButtonProps::default()
                .ok_variant(ButtonVariant::Paint(Tone::Danger, Emphasis::Solid))
                .ok_text("Clear data")
                .cancel_text("Cancel")
                .show_cancel(true),
        )
}

pub fn import_configuration_file_alert(
    alert: AlertDialog,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
) -> AlertDialog {
    alert
        .title(title.into())
        .description(description.into())
        .button_props(
            DialogButtonProps::default()
                .ok_variant(ButtonVariant::Paint(Tone::Warning, Emphasis::Solid))
                .ok_text("Import")
                .cancel_text("Cancel")
                .show_cancel(true),
        )
}

pub fn add_host_prompt_dialog(
    dialog: Dialog,
    input: &Entity<InputState>,
    error: Option<SharedString>,
    cx: &App,
) -> Dialog {
    prompt_dialog(
        dialog,
        "Add host",
        DialogButtonProps::default()
            .ok_text("Add")
            .cancel_text("Cancel")
            .show_cancel(true),
    )
    .child(
        prompt_body()
            .child(dialog_description(cx).child(ADD_HOST_DESCRIPTION))
            .child(Input::new(input))
            .when_some(error, |this, error| {
                this.child(
                    div()
                        .text_size(rems_from_px(11.0))
                        .text_color(cx.theme().warning)
                        .child(error),
                )
            }),
    )
}

pub fn ssh_secret_prompt_dialog(
    dialog: Dialog,
    title: impl Into<SharedString>,
    question: &str,
    input: &Entity<InputState>,
    cx: &App,
) -> Dialog {
    ssh_text_prompt_dialog(dialog, title, question, input, false, cx)
}

pub fn ssh_text_prompt_dialog(
    dialog: Dialog,
    title: impl Into<SharedString>,
    question: &str,
    input: &Entity<InputState>,
    echo: bool,
    cx: &App,
) -> Dialog {
    prompt_dialog(
        dialog,
        title,
        DialogButtonProps::default()
            .ok_text("Continue")
            .cancel_text("Cancel")
            .show_cancel(true),
    )
    .child(prompt_body().child(prompt_question(question, cx)).child(
        Input::new(input).when(!echo, |input| {
            input.content_type(InputContentType::Password)
        }),
    ))
}

pub fn ssh_confirm_prompt_dialog(
    dialog: Dialog,
    title: impl Into<SharedString>,
    question: &str,
    cx: &App,
) -> Dialog {
    prompt_dialog(
        dialog,
        title,
        DialogButtonProps::default()
            .ok_variant(ButtonVariant::Paint(Tone::Warning, Emphasis::Solid))
            .ok_text("Yes, connect")
            .cancel_text("No")
            .show_cancel(true),
    )
    .child(prompt_question(question, cx))
}

pub fn ssh_offer_prompt_dialog(
    dialog: Dialog,
    title: impl Into<SharedString>,
    question: &str,
    accept: impl Into<SharedString>,
    cx: &App,
) -> Dialog {
    prompt_dialog(
        dialog,
        title,
        DialogButtonProps::default()
            .ok_text(accept)
            .cancel_text("Not now")
            .show_cancel(true),
    )
    .child(prompt_question(question, cx))
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
