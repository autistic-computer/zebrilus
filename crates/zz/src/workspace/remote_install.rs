use zz_gpui::{App, Entity, Window};
use zz_ui::{Emphasis, Tone, WindowExt as _, button::ButtonVariant, dialog::DialogButtonProps};

use crate::mux::{client::MuxClient, hosts::InstallOffer};

pub(crate) fn menu_label(offer: InstallOffer) -> &'static str {
    match offer {
        InstallOffer::Install => "Install zz",
        InstallOffer::Update => "Update zz",
    }
}

fn update_copy(names: &[String]) -> (String, String) {
    let version = env!("CARGO_PKG_VERSION");
    let hosts = match names {
        [] => "these hosts".to_owned(),
        [name] => name.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    let (runs, daemons, them) = if names.len() == 1 {
        ("runs", "its daemon", "it")
    } else {
        ("run", "their daemons", "them")
    };
    (
        format!("Update zz on {hosts}?"),
        format!(
            "{hosts} {runs} an older zz. Updating installs zz {version} in ~/.local there and \
             restarts {daemons}, which ends the sessions running on {them}."
        ),
    )
}

pub(crate) fn open(mux: &Entity<MuxClient>, window: &mut Window, cx: &mut App) {
    let mux = mux.clone();
    window.open_alert_dialog(cx, move |alert, _, cx| {
        let (title, description) = update_copy(&mux.read(cx).update_offer_names());
        let accept = mux.clone();
        let close = mux.clone();
        alert
            .title(title)
            .description(description)
            .button_props(
                DialogButtonProps::default()
                    .ok_variant(ButtonVariant::Paint(Tone::Danger, Emphasis::Solid))
                    .ok_text("Update and restart")
                    .cancel_text("Not now")
                    .show_cancel(true),
            )
            .on_ok(move |_, _, cx| {
                accept.update(cx, MuxClient::accept_update_offers);
                true
            })
            .on_close(move |_, _, cx| {
                close.update(cx, |mux, _| mux.clear_update_offers());
            })
    });
}

#[cfg(test)]
mod tests {
    use super::update_copy;

    #[test]
    fn one_dialog_names_every_host_it_updates() {
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(
            update_copy(&["xps".to_owned()]),
            (
                "Update zz on xps?".to_owned(),
                format!(
                    "xps runs an older zz. Updating installs zz {version} in ~/.local there and \
                     restarts its daemon, which ends the sessions running on it."
                )
            )
        );
        let (title, description) = update_copy(&[
            "xps".to_owned(),
            "ubuntu".to_owned(),
            "alienware".to_owned(),
        ]);
        assert_eq!(title, "Update zz on xps, ubuntu and alienware?");
        assert!(
            description.starts_with("xps, ubuntu and alienware run an older zz.")
                && description
                    .ends_with("restarts their daemons, which ends the sessions running on them."),
            "{description}"
        );
    }
}
