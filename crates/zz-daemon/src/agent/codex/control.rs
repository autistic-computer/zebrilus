use std::{
    collections::VecDeque,
    io::ErrorKind,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

const SOCKET: &str = "app-server-control/app-server-control.sock";
const POLL: Duration = Duration::from_millis(20);

pub(crate) fn socket(home: &Path) -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    std::fs::canonicalize(home.join(SOCKET)).ok()
}

pub(crate) enum Next {
    Message(Value),
    Empty,
    Gone,
}

pub(crate) struct Connection {
    outgoing: Sender<String>,
    incoming: Receiver<Value>,
    wake: async_channel::Receiver<()>,
    poke: async_channel::Sender<()>,
    backlog: VecDeque<Value>,
    next: u64,
}

pub(crate) struct Peer {
    pub(crate) incoming: Sender<Value>,
    pub(crate) outgoing: Receiver<String>,
    wake: async_channel::Sender<()>,
}

impl Peer {
    pub(crate) fn deliver(&self, message: Value) -> bool {
        let sent = self.incoming.send(message).is_ok();
        let _ = self.wake.try_send(());
        sent
    }
}

impl Connection {
    pub(crate) fn open(path: &Path, timeout: Duration) -> Result<Self, String> {
        let stream = UnixStream::connect(path).map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(timeout))
            .and_then(|()| stream.set_write_timeout(Some(timeout)))
            .map_err(|error| error.to_string())?;
        let (socket, _) =
            tungstenite::client("ws://localhost/", stream).map_err(|error| error.to_string())?;
        socket
            .get_ref()
            .set_read_timeout(Some(POLL))
            .map_err(|error| error.to_string())?;
        let (connection, peer) = Self::pair();
        std::thread::Builder::new()
            .name("zz-codex-control".to_owned())
            .spawn(move || pump(socket, &peer))
            .map_err(|error| error.to_string())?;
        Ok(connection)
    }

    pub(crate) fn pair() -> (Self, Peer) {
        let (outgoing, sent) = crossbeam_channel::unbounded();
        let (deliver, incoming) = crossbeam_channel::unbounded();
        let (poke, wake) = async_channel::bounded(1);
        (
            Self {
                outgoing,
                incoming,
                wake,
                poke: poke.clone(),
                backlog: VecDeque::new(),
                next: 0,
            },
            Peer {
                incoming: deliver,
                outgoing: sent,
                wake: poke,
            },
        )
    }

    pub(crate) fn request(&mut self, method: &str, params: &Value) -> u64 {
        let id = self.next;
        self.next += 1;
        self.send(&json!({ "id": id, "method": method, "params": params }));
        id
    }

    pub(crate) fn notify(&self, method: &str) {
        self.send(&json!({ "method": method }));
    }

    pub(crate) fn reply(&self, id: &Value, result: &Value) {
        self.send(&json!({ "id": id, "result": result }));
    }

    fn send(&self, message: &Value) {
        let _ = self.outgoing.send(message.to_string());
    }

    pub(crate) fn call(
        &mut self,
        method: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.request(method, params);
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.incoming.recv_timeout(left) {
                Ok(message) if message["id"] == id && message.get("method").is_none() => {
                    return match message.get("error") {
                        Some(error) => Err(error_text(error)),
                        None => Ok(message["result"].clone()),
                    };
                }
                Ok(message) => self.backlog.push_back(message),
                Err(RecvTimeoutError::Timeout) => return Err(format!("{method} timed out")),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("the app server closed the connection".to_owned());
                }
            }
        }
    }

    pub(crate) fn next(&mut self) -> Next {
        if let Some(message) = self.backlog.pop_front() {
            return Next::Message(message);
        }
        match self.incoming.try_recv() {
            Ok(message) => Next::Message(message),
            Err(TryRecvError::Empty) => Next::Empty,
            Err(TryRecvError::Disconnected) => Next::Gone,
        }
    }

    pub(crate) fn wake(&self) -> async_channel::Receiver<()> {
        self.wake.clone()
    }

    pub(crate) fn poke(&self) {
        let _ = self.poke.try_send(());
    }
}

pub(crate) fn error_text(error: &Value) -> String {
    error["message"]
        .as_str()
        .unwrap_or("request failed")
        .to_owned()
}

fn pump(mut socket: WebSocket<UnixStream>, peer: &Peer) {
    loop {
        loop {
            match peer.outgoing.try_recv() {
                Ok(text) => {
                    if socket.send(Message::text(text)).is_err() {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let _ = socket.close(None);
                    let _ = socket.flush();
                    return;
                }
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Ok(message) = serde_json::from_str::<Value>(text.as_str())
                    && !peer.deliver(message)
                {
                    return;
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
    }
}
