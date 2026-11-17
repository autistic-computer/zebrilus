use ala::{App, Entity, Window};
use penna::{
    Emphasis, Tone, WindowExt as _, blocks::dialog::DialogButtonProps,
    components::button::ButtonVariant,
};

use crate::mux::client::MuxClient;

pub(crate) fn open(mux: &Entity<MuxClient>, window: &mut Window, cx: &mut App) {
    let mux = mux.clone();
    window.open_alert_dialog(cx, move |alert, _, cx| {
        let (title, description) =
            zz_daemon_client::update_prompt(&mux.read(cx).update_offer_hosts());
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
