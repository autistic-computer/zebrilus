use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};
use zz_daemon_client::{
    AskpassPromptKind, AskpassReply, Endpoint, HostFailure, InteractiveClient, RemoteInstall,
    SshPrompts,
};
use zz_protocol::ProtocolMessage;
use zz_terminal::TerminalColorScheme;

pub struct Prompt {
    pub kind: AskpassPromptKind,
    pub text: String,
    pub echo: bool,
    pub reply: Sender<AskpassReply>,
}

pub struct Notice {
    pub kind: AskpassPromptKind,
    pub text: String,
}

pub enum Event {
    Connected(Arc<InteractiveClient>),
    Message(Box<ProtocolMessage>),
    Prompt(Prompt),
    Notice(Notice),
    Failed(HostFailure),
    Installed(Result<RemoteInstall, String>),
}

pub struct Connection {
    pub events: Receiver<Event>,
    pub ready: Option<futures::channel::mpsc::UnboundedReceiver<()>>,
    client: Arc<Mutex<Option<Arc<InteractiveClient>>>>,
    cancelled: Arc<AtomicBool>,
}

impl Connection {
    #[must_use]
    pub fn connect(endpoint: String, session: Option<String>, native_ui: bool) -> Self {
        let (tx, events) = mpsc::sync_channel(64);
        let (ready_tx, ready) = futures::channel::mpsc::unbounded();
        let tx = Notifying {
            events: tx,
            ready: ready_tx,
        };
        let client = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));
        let shared_client = client.clone();
        let cancel = cancelled.clone();
        std::thread::spawn(move || {
            let run = || -> Result<(), HostFailure> {
                let endpoint =
                    Endpoint::parse(&endpoint).map_err(|error| HostFailure::Unreachable {
                        reason: error.to_string(),
                    })?;
                let prompts = prompts(&tx, &cancel);
                let connected = Arc::new(
                    InteractiveClient::connect_endpoint_with_prompts_and_attach(
                        &endpoint,
                        Some(TerminalColorScheme::Dark),
                        Some(prompts),
                        &[
                            zz_protocol::ClientHello::CLIENT_PATH_PICKER_CAPABILITY,
                            zz_protocol::AGENT_LOGIN_CAPABILITY,
                        ],
                        session.map(zz_protocol::AttachOperation::Session),
                        !native_ui,
                    )
                    .map_err(|error| HostFailure::classify(&error))?,
                );
                {
                    let mut slot = shared_client.lock().unwrap();
                    if cancel.load(Ordering::Acquire) {
                        let _ = connected.shutdown();
                        return Ok(());
                    }
                    *slot = Some(connected.clone());
                }
                if tx.send(Event::Connected(connected.clone())).is_err() {
                    return Ok(());
                }
                while !cancel.load(Ordering::Acquire) {
                    let message = connected.recv().map_err(|error| HostFailure::Unreachable {
                        reason: error.to_string(),
                    })?;
                    if tx.send(Event::Message(Box::new(message))).is_err() {
                        break;
                    }
                }
                Ok(())
            };
            if let Err(error) = run() {
                let _ = tx.send(Event::Failed(error));
            }
            if let Some(client) = shared_client.lock().unwrap().take() {
                let _ = client.shutdown();
            }
        });
        Self {
            events,
            ready: Some(ready),
            client,
            cancelled,
        }
    }

    #[must_use]
    pub fn install(endpoint: String, restart_daemon: bool) -> Self {
        let (tx, events) = mpsc::sync_channel(64);
        let (ready_tx, ready) = futures::channel::mpsc::unbounded();
        let tx = Notifying {
            events: tx,
            ready: ready_tx,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = cancelled.clone();
        std::thread::spawn(move || {
            let result = match Endpoint::parse(&endpoint) {
                Ok(Endpoint::Ssh(ssh)) => zz_daemon_client::install_remote(
                    &ssh,
                    Some(prompts(&tx, &cancel)),
                    restart_daemon,
                )
                .map_err(|error| error.ssh_reason().unwrap_or_else(|| error.to_string())),
                Ok(_) => Err("zz installs only on ssh hosts".to_owned()),
                Err(error) => Err(error.to_string()),
            };
            let _ = tx.send(Event::Installed(result));
        });
        Self {
            events,
            ready: Some(ready),
            client: Arc::new(Mutex::new(None)),
            cancelled,
        }
    }
}

fn prompts(tx: &Notifying, cancel: &Arc<AtomicBool>) -> SshPrompts {
    let tx = tx.clone();
    let cancel = cancel.clone();
    SshPrompts::new(PathBuf::new(), move |prompt| {
        if cancel.load(Ordering::Acquire) {
            return AskpassReply::Cancel;
        }
        if prompt.kind().is_notice() {
            let _ = tx.send(Event::Notice(Notice {
                kind: prompt.kind(),
                text: prompt.text().to_owned(),
            }));
            return AskpassReply::Cancel;
        }
        let (reply, answer) = mpsc::channel();
        if tx
            .send(Event::Prompt(Prompt {
                kind: prompt.kind(),
                text: prompt.text().to_owned(),
                echo: prompt.echo(),
                reply,
            }))
            .is_err()
        {
            return AskpassReply::Cancel;
        }
        loop {
            match answer.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(reply) => return reply,
                Err(mpsc::RecvTimeoutError::Disconnected) => return AskpassReply::Cancel,
                Err(mpsc::RecvTimeoutError::Timeout) if cancel.load(Ordering::Acquire) => {
                    return AskpassReply::Cancel;
                }
                _ => {}
            }
        }
    })
}

#[derive(Clone)]
struct Notifying {
    events: mpsc::SyncSender<Event>,
    ready: futures::channel::mpsc::UnboundedSender<()>,
}

impl Notifying {
    fn send(&self, event: Event) -> Result<(), mpsc::SendError<Event>> {
        self.events.send(event)?;
        let _ = self.ready.unbounded_send(());
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(client) = self.client.lock().unwrap().take() {
            let _ = client.shutdown();
        }
    }
}
