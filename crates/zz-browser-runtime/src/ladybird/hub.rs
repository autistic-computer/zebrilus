use super::{
    messages::{client, server},
    wire::{Fields, Message, Reader, invalid, send},
};
use ala_cef::{BrowserEvent, FrameMailbox, JsDialogKind, JsDialogRequest, SessionId};
use async_channel::Sender;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io,
    net::Shutdown,
    os::{
        fd::OwnedFd,
        unix::{fs::FileExt, net::UnixStream},
    },
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};
use zz_daemon_client::unmasked::SpawnUnmasked as _;

pub(super) struct Hub {
    queue: mpsc::Sender<Work>,
    socket: UnixStream,
}

pub(super) struct Page {
    pub id: SessionId,
    pub events: Sender<BrowserEvent>,
    pub mailbox: FrameMailbox,
    pub url: Arc<Mutex<String>>,
    stores: Stores,
    view: Option<u64>,
    request: Vec<u8>,
    pending: Vec<(u32, Vec<u8>)>,
    count: Arc<AtomicU64>,
}

impl Page {
    pub fn new(
        id: SessionId,
        scale: f64,
        fps: i32,
        events: Sender<BrowserEvent>,
        mailbox: FrameMailbox,
        url: Arc<Mutex<String>>,
        count: Arc<AtomicU64>,
    ) -> Self {
        let mut request = vec![0];
        request.extend(scale.to_le_bytes());
        request.extend(f64::from(fps).to_le_bytes());
        request.extend([0, 0, 0, 0, 0, 0, 1]);
        count.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            events,
            mailbox,
            url,
            stores: Stores::default(),
            view: None,
            request,
            pending: Vec::new(),
            count,
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::Relaxed);
        let _ = self
            .events
            .try_send(BrowserEvent::Closed { session: self.id });
    }
}

enum Work {
    Open(Box<Page>),
    Command(SessionId, u32, Vec<u8>),
    Close(SessionId),
    FromEngine(Message),
    End(io::Error),
}

impl Hub {
    pub fn launch(path: &Path, profile: &Path) -> io::Result<Self> {
        let (socket, remote) = UnixStream::pair()?;
        let mut reader = Reader::new(socket.try_clone()?);
        let writer = socket.try_clone()?;
        let (queue, work) = mpsc::channel();
        std::fs::create_dir_all(profile)?;
        let path_digest = Sha256::digest(profile.canonicalize()?.as_os_str().as_encoded_bytes());
        let name = format!(
            "zz-{:016x}",
            u64::from_le_bytes(path_digest[..8].try_into().map_err(|_| invalid())?)
        );
        let mut child = Command::new(path)
            .arg("--profile")
            .arg(name)
            .env("XDG_CONFIG_HOME", profile.join("config"))
            .env("XDG_DATA_HOME", profile.join("data"))
            .env("XDG_CACHE_HOME", profile.join("cache"))
            .args(["--force-cpu-painting"])
            .arg(format!("--ui-pid={}", std::process::id()))
            .env("SOCKET_TAKEOVER", "ipc:0")
            .stdin(Stdio::from(OwnedFd::from(remote)))
            .stdout(Stdio::null())
            .spawn_unmasked()?;
        let incoming = queue.clone();
        thread::spawn(move || {
            loop {
                match reader.next() {
                    Ok(msg) => {
                        if incoming.send(Work::FromEngine(msg)).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = incoming.send(Work::End(err));
                        break;
                    }
                }
            }
        });
        thread::spawn(move || {
            let mut dispatch = Dispatch {
                socket: writer,
                pages: HashMap::new(),
                views: HashMap::new(),
                creating: VecDeque::new(),
                ready: false,
            };
            if let Err(err) = dispatch.run(work)
                && !dispatch.pages.is_empty()
            {
                log::warn!("Ladybird Hub: {err}");
                let status: Arc<str> = err.to_string().into();
                for page in dispatch.pages.values() {
                    let _ = page.events.try_send(BrowserEvent::RenderProcessTerminated {
                        session: page.id,
                        status: Arc::clone(&status),
                        error_code: -1,
                    });
                }
            }
            let _ = dispatch.socket.shutdown(Shutdown::Both);
            drop(dispatch);
            for _ in 0..100 {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(std::time::Duration::from_millis(20));
            }
            let _ = child.kill();
            let _ = child.wait();
        });
        Ok(Self { queue, socket })
    }
    pub fn open(&self, page: Page) -> io::Result<()> {
        self.queue
            .send(Work::Open(Box::new(page)))
            .map_err(|_| io::ErrorKind::BrokenPipe.into())
    }
    pub fn command(&self, id: SessionId, message: u32, body: Vec<u8>) {
        let _ = self.queue.send(Work::Command(id, message, body));
    }
    pub fn close(&self, id: SessionId) {
        let _ = self.queue.send(Work::Close(id));
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

struct Dispatch {
    socket: UnixStream,
    pages: HashMap<SessionId, Box<Page>>,
    views: HashMap<u64, SessionId>,
    creating: VecDeque<SessionId>,
    ready: bool,
}

impl Dispatch {
    fn run(&mut self, work: mpsc::Receiver<Work>) -> io::Result<()> {
        send(&mut self.socket, server::INITIALIZE, &[])?;
        for item in work {
            match item {
                Work::Open(page) => {
                    self.creating.push_back(page.id);
                    if self.ready {
                        send(&mut self.socket, server::CREATE_VIEW, &page.request)?;
                    }
                    self.pages.insert(page.id, page);
                }
                Work::Close(id) => {
                    if let Some(page) = self.pages.remove(&id) {
                        if let Some(view) = page.view {
                            self.views.remove(&view);
                            send(&mut self.socket, server::DESTROY_VIEW, &view.to_le_bytes())?;
                        } else if !self.ready {
                            self.creating.retain(|entry| *entry != id);
                        }
                    }
                }
                Work::Command(id, msg, data) => {
                    if let Some(page) = self.pages.get_mut(&id) {
                        if let Some(view) = page.view {
                            self.send_view(view, msg, &data)?;
                        } else {
                            page.pending.push((msg, data));
                        }
                    }
                }
                Work::FromEngine(msg) => self.receive_engine_message(msg)?,
                Work::End(err) => return Err(err),
            }
        }
        Ok(())
    }
    fn send_view(&mut self, view: u64, msg: u32, data: &[u8]) -> io::Result<()> {
        let mut body = view.to_le_bytes().to_vec();
        body.extend_from_slice(data);
        send(&mut self.socket, msg, &body)
    }
    fn receive_engine_message(&mut self, msg: Message) -> io::Result<()> {
        let mut body = &msg.payload[8..];
        if msg.endpoint == server::MAGIC {
            match msg.id {
                server::INITIALIZE_RESPONSE => {
                    self.ready = true;
                    for id in &self.creating {
                        send(
                            &mut self.socket,
                            server::CREATE_VIEW,
                            &self.pages[id].request,
                        )?;
                    }
                    send(&mut self.socket, server::UI_DID_START, &[])?;
                }
                server::CREATE_VIEW_RESPONSE => {
                    let view = body.u64()?;
                    let id = self.creating.pop_front().ok_or_else(invalid)?;
                    if let Some(page) = self.pages.get_mut(&id) {
                        page.view = Some(view);
                        self.views.insert(view, id);
                        let _ = page.events.try_send(BrowserEvent::Created { session: id });
                        for (msg, data) in std::mem::take(&mut page.pending) {
                            self.send_view(view, msg, &data)?;
                        }
                    } else {
                        send(&mut self.socket, server::DESTROY_VIEW, &view.to_le_bytes())?;
                    }
                }
                _ => (),
            }
        } else if msg.endpoint == client::MAGIC
            && body.len() >= 8
            && let Some(id) = self.views.get(&body.u64()?)
            && let Some(page) = self.pages.get_mut(id)
            && let Some(event) =
                page.stores
                    .handle(msg, *id, &mut self.socket, &page.mailbox, &page.url)?
        {
            let _ = page.events.try_send(event);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Stores {
    buffers: HashMap<u32, Bitmap>,
    front: Option<u32>,
    page: u64,
    navigation: Option<(bool, bool, bool)>,
}

struct Bitmap {
    file: File,
    width: usize,
    height: usize,
}

impl Bitmap {
    fn copy(&self, width: usize, height: usize) -> io::Result<Vec<u8>> {
        if width == 0 || height == 0 || width > self.width || height > self.height {
            return Err(invalid());
        }
        let stride = self.width * 4;
        let row = width * 4;
        let mut data = vec![0; stride * (height - 1) + row];
        self.file.read_exact_at(&mut data, 0)?;
        if stride != row {
            for y in 1..height {
                data.copy_within(y * stride..y * stride + row, y * row);
            }
            data.truncate(row * height);
        }
        Ok(data)
    }
}

impl Stores {
    fn handle(
        &mut self,
        msg: Message,
        session: SessionId,
        writer: &mut UnixStream,
        mailbox: &FrameMailbox,
        url: &Mutex<String>,
    ) -> io::Result<Option<BrowserEvent>> {
        let mut body = &msg.payload[16..];
        let event = match msg.id {
            client::VIEW_INSTALL_BACKING_STORES | client::VIEW_ADD_BACKING_STORES => {
                let install = msg.id == client::VIEW_INSTALL_BACKING_STORES;
                if install {
                    self.page = body.u64()?;
                    self.buffers.clear();
                    self.front = None;
                }
                let count = body.u32()? as usize;
                if count != msg.files.len() || count > 32 {
                    return Err(invalid());
                }
                let ids = (0..count)
                    .map(|_| body.u32())
                    .collect::<io::Result<Vec<_>>>()?;
                if body.u32()? as usize != count {
                    return Err(invalid());
                }
                if install {
                    self.front = ids.first().copied();
                }
                for (id, file) in ids.into_iter().zip(msg.files) {
                    if body.field(2)? != [0, 1] {
                        return Err(invalid());
                    }
                    let width = body.u32()? as usize;
                    let height = body.u32()? as usize;
                    let format = body.u32()?;
                    let _alpha = body.u32()?;
                    if !(1..=16384).contains(&width)
                        || !(1..=16384).contains(&height)
                        || width * height > 32 * 1024 * 1024
                        || !matches!(format, 1 | 2)
                    {
                        return Err(invalid());
                    }
                    self.buffers.insert(
                        id,
                        Bitmap {
                            file,
                            width,
                            height,
                        },
                    );
                }
                return Ok(None);
            }
            client::VIEW_RETIRE_BACKING_STORES => {
                for _ in 0..body.u32()? {
                    self.buffers.remove(&body.u32()?);
                }
                return Ok(None);
            }
            client::VIEW_CLEAR_BACKING_STORES => {
                self.front = None;
                self.buffers.clear();
                return Ok(None);
            }
            client::VIEW_PRESENT_BACKING_STORE => {
                let page = body.u64()?;
                let id = body.u32()?;
                let width = body.u32()?;
                let height = body.u32()?;
                let mut generation = None;
                let mut release = id;
                if page == self.page
                    && let Some(bitmap) = self.buffers.get(&id)
                {
                    let pixels = bitmap.copy(width as usize, height as usize)?;
                    generation = mailbox
                        .publish(session, width as i32, height as i32, pixels, None)
                        .map_err(io::Error::other)?;
                    release = self.front.replace(id).unwrap_or(id);
                }
                let mut response = page.to_le_bytes().to_vec();
                response.extend(release.to_le_bytes());
                send(writer, server::RELEASE_BACKING_STORE, &response)?;
                return Ok(generation.map(|generation| BrowserEvent::FrameReady {
                    session,
                    generation,
                }));
            }
            client::VIEW_REQUEST_ALERT
            | client::VIEW_REQUEST_CONFIRM
            | client::VIEW_REQUEST_PROMPT => {
                let message = body.title()?.into();
                let (kind, response, default_prompt) = match msg.id {
                    client::VIEW_REQUEST_ALERT => {
                        (JsDialogKind::Alert, server::VIEW_ALERT_CLOSED, None)
                    }
                    client::VIEW_REQUEST_CONFIRM => {
                        (JsDialogKind::Confirm, server::VIEW_CONFIRM_CLOSED, None)
                    }
                    _ => (
                        JsDialogKind::Prompt,
                        server::VIEW_PROMPT_CLOSED,
                        Some(body.title()?.into()),
                    ),
                };
                BrowserEvent::JsDialogRequested {
                    session,
                    request: JsDialogRequest {
                        id: u64::from(response),
                        kind,
                        origin_url: url.lock().as_str().into(),
                        message,
                        default_prompt,
                    },
                }
            }
            client::VIEW_URL_CHANGE => {
                let next = body.string()?;
                next.clone_into(&mut url.lock());
                BrowserEvent::AddressChanged {
                    session,
                    url: next.into(),
                }
            }
            client::VIEW_TITLE_CHANGE => BrowserEvent::TitleChanged {
                session,
                title: body.title()?.into(),
            },
            client::VIEW_STATE_CHANGED => {
                for _ in 0..2 {
                    body.string()?;
                    if body.byte()? != 0 {
                        return Ok(None);
                    }
                }
                body.title()?;
                body.string()?;
                let loading = body.byte()? != 0;
                body.byte()?;
                body.string()?;
                body.byte()?;
                for _ in 0..2 {
                    if body.byte()? != 0 {
                        body.string()?;
                    }
                }
                body.field(8)?;
                body.string()?;
                body.field(8)?;
                let back = body.byte()? != 0;
                let forward = body.byte()? != 0;
                if self.navigation.replace((loading, back, forward))
                    == Some((loading, back, forward))
                {
                    return Ok(None);
                }
                BrowserEvent::LoadingChanged {
                    session,
                    loading,
                    can_go_back: back,
                    can_go_forward: forward,
                }
            }
            client::VIEW_CRASH_OVERLAY_STATE_CHANGE if body.byte()? != 0 => {
                BrowserEvent::RenderProcessTerminated {
                    session,
                    status: "Ladybird WebContent exited".into(),
                    error_code: -1,
                }
            }
            client::VIEW_CLOSE => BrowserEvent::Closed { session },
            _ => return Ok(None),
        };
        Ok(Some(event))
    }
}
