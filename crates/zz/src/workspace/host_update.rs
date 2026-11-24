use ala::{App, Entity, Window};
use penna::{WindowExt as _, blocks::dialog::DialogButtonProps, components::button::Button};

use crate::mux::client::{HostUpdateQuestion, MuxClient};

pub(crate) fn ask(
    mux: &Entity<MuxClient>,
    question: &HostUpdateQuestion,
    window: &mut Window,
    cx: &mut App,
) {
    let names = question
        .hosts
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let ((title, description), accept) = match &question.installed {
        Some(installed) => (
            zz_daemon_client::host_restart_question(&question.hosts, installed),
            "Restart",
        ),
        None => (
            zz_daemon_client::host_update_question(&question.hosts),
            "Update and restart",
        ),
    };
    let install_only = question.installed.is_none();
    let mux = mux.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let run = |restart: bool| {
            let mux = mux.clone();
            let names = names.clone();
            move |cx: &mut App| mux.update(cx, |mux, cx| mux.run_host_updates(&names, restart, cx))
        };
        let (restart, install) = (run(true), run(false));
        let dismiss_mux = mux.clone();
        let dismissed = names.clone();
        let alert = alert
            .title(title.clone())
            .description(description.clone())
            .button_props(
                DialogButtonProps::default()
                    .ok_text(accept)
                    .cancel_text("Not now")
                    .show_cancel(true),
            )
            .on_ok(move |_, _, cx| {
                restart(cx);
                true
            })
            .on_close(move |_, _, cx| {
                dismiss_mux.update(cx, |mux, cx| mux.dismiss_host_updates(&dismissed, cx));
            });
        if !install_only {
            return alert;
        }
        alert.action(
            Button::new("host-update-install-only")
                .label("Install only")
                .on_click(move |_, window, cx| {
                    install(cx);
                    window.close_dialog(cx);
                }),
        )
    });
}
