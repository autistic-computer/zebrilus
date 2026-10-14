use zz_gpui::{App, Entity, Window};
use zz_ui::{Emphasis, Tone, WindowExt as _, button::ButtonVariant, dialog::DialogButtonProps};

use crate::mux::{
    client::MuxClient,
    hosts::{HostId, InstallOffer},
};

pub(crate) fn menu_label(offer: InstallOffer) -> &'static str {
    match offer {
        InstallOffer::Install => "Install zz",
        InstallOffer::Update => "Update zz",
    }
}

pub(crate) fn open(
    mux: &Entity<MuxClient>,
    host: HostId,
    name: &str,
    offer: InstallOffer,
    window: &mut Window,
    cx: &mut App,
) {
    let version = env!("CARGO_PKG_VERSION");
    let (title, description, ok, tone) = match offer {
        InstallOffer::Install => (
            format!("Install zz on {name}?"),
            format!(
                "zz is not installed on {name}. This runs the zz installer there over ssh and puts \
                 zz {version} in ~/.local/bin."
            ),
            "Install",
            Tone::Primary,
        ),
        InstallOffer::Update => (
            format!("Update zz on {name}?"),
            format!(
                "{name} runs an older zz. Updating puts zz {version} in ~/.local/bin there and \
                 restarts its daemon, which ends the sessions running on it."
            ),
            "Update and restart",
            Tone::Danger,
        ),
    };
    let mux = mux.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let mux = mux.clone();
        alert
            .title(title.clone())
            .description(description.clone())
            .button_props(
                DialogButtonProps::default()
                    .ok_variant(ButtonVariant::Paint(tone, Emphasis::Solid))
                    .ok_text(ok)
                    .cancel_text("Not now")
                    .show_cancel(true),
            )
            .on_ok(move |_, _, cx| {
                mux.update(cx, |mux, cx| mux.install_zz_on_host(host, cx));
                true
            })
    });
}
