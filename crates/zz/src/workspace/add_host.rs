use ala::{App, AppContext as _, Entity, Focusable as _, SharedString, Window};
use penna::{WindowExt as _, components::input::InputState};
use zz_ui::dialogs::add_host_prompt_dialog;

use crate::config;

pub(crate) use zz_daemon_client::{ADD_HOST_PLACEHOLDER as PLACEHOLDER, parse_add_host};

struct AddHostDialogState {
    error: Option<SharedString>,
}

pub fn open(window: &mut Window, cx: &mut App) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
    let state = cx.new(|_| AddHostDialogState { error: None });

    let dialog_input = input.clone();
    let dialog_state = state.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
        let error = dialog_state.read(cx).error.clone();
        let submit_input = dialog_input.clone();
        let submit_state = dialog_state.clone();
        add_host_prompt_dialog(dialog, &dialog_input, error, cx)
            .on_ok(move |_, window, cx| submit(&submit_input, &submit_state, window, cx))
    });
    input.read(cx).focus_handle(cx).focus(window, cx);
}

fn submit(
    input: &Entity<InputState>,
    state: &Entity<AddHostDialogState>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let value = input.read(cx).value();
    let existing = config::fleet_hosts(cx)
        .into_iter()
        .map(|host| host.name)
        .collect::<Vec<_>>();
    let request = match parse_add_host(&value, &existing) {
        Ok(request) => request,
        Err(message) => return fail(state, message, window, cx),
    };
    if let Err(error) = config::add_fleet_host(&request.name, &request.endpoint, cx) {
        return fail(
            state,
            format!("Could not write zz/config: {error}"),
            window,
            cx,
        );
    }
    log::info!(
        target: "zz::config",
        "added fleet host name={} endpoint={}",
        request.name,
        request.endpoint,
    );
    true
}

fn fail(
    state: &Entity<AddHostDialogState>,
    message: String,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    state.update(cx, |state, cx| {
        state.error = Some(message.into());
        cx.notify();
    });
    window.refresh();
    false
}
