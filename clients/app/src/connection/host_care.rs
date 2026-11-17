use ala::Context;
use zz_daemon_client::{
    AskpassPromptKind, DaemonBehind, Endpoint, HostFailure, InstallOffer, RemoteInstall,
};

use super::Connection;
use crate::transport::{self, Event};

pub enum HostNotice {
    KeyAdded,
    KeyFailed(String),
    Installed(RemoteInstall),
    InstallFailed(String),
}

impl ala::EventEmitter<HostNotice> for Connection {}

impl Connection {
    #[must_use]
    pub fn daemon_behind(&self) -> Option<DaemonBehind> {
        let client = self.client.as_ref().filter(|_| self.connected)?;
        DaemonBehind::compare(client.server_hello().revision, zz_protocol::REVISION)
    }

    #[must_use]
    pub fn session_count(&self) -> Option<usize> {
        self.connected.then(|| self.core.snapshot().sessions.len())
    }

    #[must_use]
    pub fn install_offer(&self) -> Option<InstallOffer> {
        if self.installer.is_some() || !self.on_ssh() {
            return None;
        }
        self.failure
            .as_ref()
            .and_then(HostFailure::install_offer)
            .or_else(|| self.daemon_behind().map(|_| InstallOffer::Update))
    }

    fn on_ssh(&self) -> bool {
        self.endpoint
            .as_deref()
            .and_then(|endpoint| Endpoint::parse(endpoint).ok())
            .is_some_and(|endpoint| matches!(endpoint, Endpoint::Ssh(_)))
    }

    pub fn install_zz(&mut self, cx: &mut Context<Self>) {
        let (Some(offer), Some(endpoint)) = (self.install_offer(), self.endpoint.clone()) else {
            return;
        };
        let mut installer = transport::Connection::install(endpoint, offer == InstallOffer::Update);
        let ready = installer.ready.take();
        self.installer = Some(installer);
        if !self.connected {
            self.status = match offer {
                InstallOffer::Install => "Installing zz…",
                InstallOffer::Update => "Updating zz…",
            }
            .into();
        }
        if let Some(mut ready) = ready {
            cx.spawn(async move |this, cx| {
                use futures::StreamExt as _;
                while ready.next().await.is_some() {
                    while ready.try_recv().is_ok() {}
                    if this.update(cx, Connection::poll_installer).unwrap_or(true) {
                        return;
                    }
                }
            })
            .detach();
        }
        cx.notify();
    }

    fn poll_installer(&mut self, cx: &mut Context<Self>) -> bool {
        loop {
            let Some(event) = self
                .installer
                .as_ref()
                .map(|installer| installer.events.try_recv())
            else {
                return true;
            };
            let Ok(event) = event else {
                return false;
            };
            match event {
                Event::Prompt(prompt) => self.ask(prompt, cx),
                Event::Notice(notice) => self.notice(notice.kind, notice.text, cx),
                Event::Installed(result) => {
                    self.installer = None;
                    match result {
                        Ok(outcome) => {
                            cx.emit(HostNotice::Installed(outcome));
                            if !self.connected {
                                self.reconnect(cx);
                            }
                        }
                        Err(reason) => {
                            if !self.connected {
                                self.status = self
                                    .failure
                                    .as_ref()
                                    .map_or_else(|| "Not connected".to_owned(), failure_status);
                            }
                            cx.emit(HostNotice::InstallFailed(reason));
                        }
                    }
                    cx.notify();
                    return true;
                }
                Event::Connected(_) | Event::Message(_) | Event::Failed(_) => {}
            }
        }
    }

    pub(super) fn notice(&mut self, kind: AskpassPromptKind, text: String, cx: &mut Context<Self>) {
        match kind {
            AskpassPromptKind::KeyAdded => cx.emit(HostNotice::KeyAdded),
            AskpassPromptKind::KeyFailed => cx.emit(HostNotice::KeyFailed(text)),
            _ => {}
        }
    }

    pub(super) fn fail_native(&mut self, failure: HostFailure, cx: &mut Context<Self>) {
        if !matches!(failure, HostFailure::Unreachable { .. }) {
            self.resume = false;
        }
        self.disconnect_native(failure_status(&failure), cx);
        self.failure = Some(failure);
    }
}

fn failure_status(failure: &HostFailure) -> String {
    if let HostFailure::Unreachable { reason } = failure {
        return reason.clone();
    }
    let step = failure.step().unwrap_or(match failure {
        HostFailure::DaemonTooOld { .. } => "Update zz there, then reconnect.",
        HostFailure::ClientTooOld { .. } => "Update this app, then reconnect.",
        _ => "Install zz there, then reconnect.",
    });
    format!("{}\n{step}", failure.summary())
}
