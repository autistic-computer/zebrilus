//! The dialog ssh's password and host-key questions appear in.

use std::{cell::Cell, rc::Rc, thread, time::Duration};

use ala::{App, AppContext as _, Entity, Focusable as _, Window};
use zz_daemon_client::{AskpassPromptKind, AskpassReply};
use zz_ui::{
    WindowExt as _,
    feedback::{ssh_confirm_prompt_dialog, ssh_secret_prompt_dialog},
    input::InputState,
    notification::Notification,
};

use crate::mux::{
    client::{MuxClient, SshPromptRequest},
    hosts::HostId,
};

const TOUCH_NOTICE_SECONDS: u64 = 15;

pub(crate) fn open(
    mux: &Entity<MuxClient>,
    request: &SshPromptRequest,
    window: &mut Window,
    cx: &mut App,
) {
    match request.prompt.kind() {
        AskpassPromptKind::Secret => open_secret(mux, request, window, cx),
        AskpassPromptKind::HostKey
        | AskpassPromptKind::AgentConfirm
        | AskpassPromptKind::SaveKey => {
            open_confirmation(mux, request, window, cx);
        }
        AskpassPromptKind::Notification => window.push_notification(
            Notification::info(format!(
                "Touch your security key to sign in to {}",
                request.label
            ))
            .key(format!("ssh-touch-{}", request.label))
            .autohide_after(Duration::from_secs(TOUCH_NOTICE_SECONDS)),
            cx,
        ),
        AskpassPromptKind::KeyAdded => {
            window.push_notification(
                Notification::success(format!(
                    "Added zz's key to {}; it won't ask to sign in again",
                    request.label
                )),
                cx,
            );
            sync_fleet_in_background();
        }
    }
}

fn sync_fleet_in_background() {
    let spawned = thread::Builder::new()
        .name("zz-fleet-sync".to_owned())
        .spawn(|| match zz_daemon_client::sync_fleet() {
            Ok(lines) => {
                for line in lines {
                    match line.outcome {
                        Ok(text) => log::info!("fleet sync: {}: {text}", line.name),
                        Err(reason) => log::warn!("fleet sync: {}: {reason}", line.name),
                    }
                }
            }
            Err(error) => log::warn!("fleet sync: {error}"),
        });
    if let Err(error) = spawned {
        log::warn!("fleet sync: could not start: {error}");
    }
}

struct Answered {
    reply: async_channel::Sender<AskpassReply>,
    done: Rc<Cell<bool>>,
}

impl Answered {
    fn new(reply: &async_channel::Sender<AskpassReply>) -> Self {
        Self {
            reply: reply.clone(),
            done: Rc::new(Cell::new(false)),
        }
    }

    fn clone_handle(&self) -> Self {
        Self {
            reply: self.reply.clone(),
            done: Rc::clone(&self.done),
        }
    }

    fn send(&self, reply: AskpassReply) {
        if self.done.replace(true) {
            return;
        }
        let _ = self.reply.try_send(reply);
        self.reply.close();
    }
}

fn decline(
    answered: &Answered,
    mux: &Entity<MuxClient>,
    host: HostId,
    reply: AskpassReply,
    cx: &mut App,
) {
    answered.send(reply);
    mux.update(cx, |mux, cx| mux.note_ssh_auth_declined(host, cx));
}

fn open_secret(
    mux: &Entity<MuxClient>,
    request: &SshPromptRequest,
    window: &mut Window,
    cx: &mut App,
) {
    let input = cx.new(|cx| InputState::new(window, cx));
    let answered = Answered::new(&request.reply);
    let host = request.host;
    let mux = mux.clone();
    let title = format!("Sign in to {}", request.label);
    let question = request.prompt.text().trim_end().to_owned();

    let dialog_input = input.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
        let confirm_input = dialog_input.clone();
        let confirm = answered.clone_handle();
        let cancel = answered.clone_handle();
        let cancel_mux = mux.clone();
        ssh_secret_prompt_dialog(dialog, title.clone(), &question, &dialog_input, cx)
            .on_ok(move |_, _, cx| {
                confirm.send(AskpassReply::answer(
                    confirm_input.read(cx).value().as_ref(),
                ));
                true
            })
            .on_cancel(move |_, _, cx| {
                decline(&cancel, &cancel_mux, host, AskpassReply::Cancel, cx);
                true
            })
    });
    input.read(cx).focus_handle(cx).focus(window, cx);
}

fn open_confirmation(
    mux: &Entity<MuxClient>,
    request: &SshPromptRequest,
    window: &mut Window,
    cx: &mut App,
) {
    let answered = Answered::new(&request.reply);
    let host = request.host;
    let mux = mux.clone();
    let host_key = request.prompt.kind() == AskpassPromptKind::HostKey;
    let title = if host_key {
        format!("Unrecognised host key for {}", request.label)
    } else {
        format!("{} is asking for permission", request.label)
    };
    let question = request.prompt.text().trim_end().to_owned();

    window.open_dialog(cx, move |dialog, _, cx| {
        let confirm = answered.clone_handle();
        let cancel = answered.clone_handle();
        let cancel_mux = mux.clone();
        ssh_confirm_prompt_dialog(dialog, title.clone(), &question, cx)
            .on_ok(move |_, _, _| {
                confirm.send(AskpassReply::answer("yes"));
                true
            })
            .on_cancel(move |_, _, cx| {
                let reply = if host_key {
                    AskpassReply::answer("no")
                } else {
                    AskpassReply::Cancel
                };
                decline(&cancel, &cancel_mux, host, reply, cx);
                true
            })
    });
}
