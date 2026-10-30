use ala::{
    App, Entity, ParentElement as _, SharedString, Styled as _, div, prelude::FluentBuilder as _,
};
use penna::{
    dialog::{AlertDialog, Dialog, DialogButtonProps},
    feedback::{prompt_body, prompt_dialog, prompt_question},
    overlay::dialog_description,
};

use crate::{
    ActiveTheme as _, Emphasis, Icon, IconName, Tone,
    button::ButtonVariant,
    input::{Input, InputContentType, InputState},
    rems_from_px,
};

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
