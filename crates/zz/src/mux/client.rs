use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    io,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use ala::{
    App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, Image, RenderImage, Task,
    WeakEntity,
};
use ala_cef::{ArtifactFile, ArtifactReply, ArtifactRequest};
use image::{Frame as ImageFrame, ImageBuffer, Rgba};
use parking_lot::RwLock;
pub(crate) use zz_client::scrollback::{HistoryRing, RetainedTerminalViewport};
use zz_client::{
    ClientCore, CoreEvent, Outbound,
    scrollback::{
        DeferredBackfill, HISTORY_BACKFILL_QUIET, HistoryFollowUp, HistoryPacer,
        apply_history_chunk, apply_retained_patch, new_retained_viewport,
        replace_retained_viewport,
    },
};
use zz_client::{
    open_uri::{OpenUriRoute, open_uri_route},
    url_input::diagnostic_url,
};
#[cfg(test)]
use zz_daemon_client::EndpointError;
use zz_daemon_client::{
    AskpassPrompt, AskpassPromptKind, AskpassReply, DaemonError, Endpoint, InteractiveClient,
    RemoteInstall, terminate_incompatible_daemon,
};
use zz_protocol::{
    BrowserCommand, CONFIG_RELOADED_MESSAGE, ChooseBufferState, ChooseTreeState, ClientMessageKind,
    ClipboardProducer, CommandInvocation, CommandPromptState, CommandResponse, ConfirmState,
    DisplayPanesState, Event, EventPayload, GitMark, GuiResponse, InputMessage, KeyBindingSnapshot,
    KeyTableSnapshot, MenuState, MuxOptionKey, MuxSnapshot, NEW_SESSION_ATTACH_CAPABILITY, PaneId,
    PastedImageFormat, PathEntry, PathListRoot, ProtocolMessage, ServerError, ServerHello,
    SessionId, SimulatorDescriptor, TerminalUiCommand,
};
#[cfg(test)]
use zz_protocol::{PROTOCOL_VERSION, ProtocolError, StatusLine};
use zz_terminal::{
    AppearanceProvenance, ClipboardTarget, LocalColors, PackedCell, TerminalAppearance,
    TerminalColorScheme, TerminalDictionary, TerminalDiffScratch, TerminalViewAction,
    TerminalViewport, TerminalViewportPatch,
};

mod agent_streams;
mod attach;
mod commands;
mod connection;
mod daemon_restart;
mod host_update;
mod images;
mod input;
mod overlays;
mod providers;

pub(crate) use host_update::{HostUpdate, HostUpdateQuestion};

use crate::{
    diagnostics,
    mux::hosts::{DaemonBehind, HostId, HostRegistry, HostState},
    terminal::view::localize_terminal_font_families,
};

#[cfg(feature = "agent-pane")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AgentRequest {
    Prompt {
        text: String,
        images: Vec<zz_protocol::AgentImage>,
    },
    Cancel,
    Unqueue,
    RespondPermission {
        request_id: u64,
        option_id: Option<String>,
    },
    Replay {
        from_seq: u64,
    },
    AcknowledgePromptRestore {
        reclaim_id: u64,
    },
    AnswerQuestion {
        request_id: u64,
        answers: Vec<zz_protocol::AgentQuestionAnswer>,
    },
    SetConfigOption {
        option_id: String,
        value: String,
    },
    SessionOp(zz_protocol::AgentSessionOpKind),
}

#[cfg(feature = "agent-pane")]
#[derive(Default)]
pub(crate) struct AgentEvents {
    pub(crate) items: Vec<(PaneId, Vec<zz_daemon::AgentStreamItem>)>,
    pub(crate) states: Vec<(PaneId, zz_protocol::AgentPaneWire)>,
    pub(crate) sessions: Vec<(PaneId, String)>,
}

#[cfg(feature = "agent-pane")]
impl AgentEvents {
    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.states.is_empty() && self.sessions.is_empty()
    }
}

#[cfg(feature = "agent-pane")]
fn accept_agent_items(
    pane: PaneId,
    cursor: &mut u64,
    first_seq: u64,
    items: Vec<Vec<u8>>,
) -> (Vec<zz_daemon::AgentStreamItem>, bool) {
    let mut accepted = Vec::new();
    for (index, blob) in items.into_iter().enumerate() {
        let positional = first_seq.saturating_add(index as u64);
        let item = match serde_json::from_slice::<zz_daemon::AgentStreamItem>(&blob) {
            Ok(item) => item,
            Err(error) => {
                log::warn!(
                    target: "zz::diagnostics::mux",
                    "dropping undecodable agent item pane={pane} seq={positional}: {error}"
                );
                *cursor = (*cursor).max(positional);
                continue;
            }
        };
        if item.seq > cursor.saturating_add(1)
            && matches!(
                &item.payload,
                zz_daemon::AgentStreamPayload::SessionReset { restoring: true }
            )
        {
            *cursor = item.seq.saturating_sub(1);
        }
        if item.seq <= *cursor {
            continue;
        }
        if item.seq > cursor.saturating_add(1) {
            return (accepted, true);
        }
        *cursor = item.seq;
        accepted.push(item);
    }
    (accepted, false)
}

const MAX_PENDING_DECODED_MESSAGES: usize = 1;
const MAX_DECODED_MESSAGES_PER_UPDATE: usize = 64;
const MAX_DECODED_BATCH_TIME: Duration = Duration::from_millis(4);
const MAX_PANE_IMAGE_SNAPSHOTS: usize = 8;
const MAX_TRACKED_COMMANDS: usize = 32;
const TERMINAL_FONT_SIZE_STEP_POINTS: f32 = 1.0;
const MIN_TERMINAL_FONT_SIZE_POINTS: f32 = 1.0;
const MAX_TERMINAL_FONT_SIZE_POINTS: f32 = 256.0;

fn new_session_commands(
    host: HostId,
    capabilities: &[String],
    working_directory: Option<&Path>,
) -> Vec<CommandInvocation> {
    if host != HostId::LOCAL {
        return vec![CommandInvocation::new("new-session", ["-d"])];
    }
    let arguments = working_directory.map_or_else(Vec::new, |directory| {
        vec!["-c".to_owned(), directory.to_string_lossy().into_owned()]
    });
    let mut commands = vec![CommandInvocation::new("new-session", arguments)];
    if !capabilities
        .iter()
        .any(|capability| capability == NEW_SESSION_ATTACH_CAPABILITY)
    {
        commands.push(CommandInvocation::new("attach-session", [] as [&str; 0]));
    }
    commands
}

fn snapshot_contains_pane(snapshot: &MuxSnapshot, pane: PaneId) -> bool {
    snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .any(|window| window.panes.contains_key(&pane))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalFontSizeAdjustment {
    Increase,
    Decrease,
}

impl TerminalFontSizeAdjustment {
    const fn delta_points(self) -> f32 {
        match self {
            Self::Increase => TERMINAL_FONT_SIZE_STEP_POINTS,
            Self::Decrease => -TERMINAL_FONT_SIZE_STEP_POINTS,
        }
    }
}

const DIAGNOSTIC_TARGET: &str = "zz::diagnostics::mux";
const MAX_SIGN_IN_RECONNECT_ATTEMPTS: u32 = 3;

fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        0 | 1 => 1,
        2 => 2,
        3 => 4,
        4 => 8,
        5 => 16,
        _ => 30,
    })
}

fn decoded_message_channel() -> (
    async_channel::Sender<ProtocolMessage>,
    async_channel::Receiver<ProtocolMessage>,
) {
    async_channel::bounded(MAX_PENDING_DECODED_MESSAGES)
}

fn drain_decoded_batch<T>(
    first: T,
    incoming: &async_channel::Receiver<T>,
    started: Instant,
    mut handle: impl FnMut(T) -> bool,
) -> Option<bool> {
    let mut next = Some(first);
    let mut handled = 0;
    while let Some(message) = next.take() {
        if !handle(message) {
            return None;
        }
        handled += 1;
        if handled == MAX_DECODED_MESSAGES_PER_UPDATE || started.elapsed() >= MAX_DECODED_BATCH_TIME
        {
            return Some(true);
        }
        next = incoming.try_recv().ok();
    }
    Some(false)
}

fn connect_error_state(error: &DaemonError) -> HostState {
    zz_daemon_client::HostFailure::classify(error).into()
}

#[cfg(test)]
fn connect_result_state<T>(result: &Result<T, DaemonError>) -> HostState {
    match result {
        Ok(_) => HostState::Connected,
        Err(error) => connect_error_state(error),
    }
}

fn reconnect_timer_is_current(connection: &HostConnection, generation: u64, attempt: u32) -> bool {
    connection.reconnect_generation == generation
        && connection.state == (HostState::Reconnecting { attempt })
}

#[derive(Default)]
struct PaneImageSnapshots {
    images: BTreeMap<u32, Arc<Image>>,
    order: VecDeque<u32>,
    revision: u64,
}

impl PaneImageSnapshots {
    fn insert(&mut self, number: u32, image: Arc<Image>) {
        self.images.insert(number, image);
        self.order.retain(|stored| *stored != number);
        self.order.push_back(number);
        while self.images.len() > MAX_PANE_IMAGE_SNAPSHOTS {
            if let Some(oldest) = self.order.pop_front() {
                self.images.remove(&oldest);
            }
        }
        self.revision = self.revision.wrapping_add(1).max(1);
    }

    fn get(&self, number: u32) -> Option<Arc<Image>> {
        self.images.get(&number).cloned()
    }

    fn remove(&mut self, number: u32) {
        if self.images.remove(&number).is_some() {
            self.order.retain(|stored| *stored != number);
            self.revision = self.revision.wrapping_add(1).max(1);
        }
    }
}

pub(crate) enum PathPickerUpdate {
    Open {
        pane: PaneId,
        start_dir: Option<String>,
    },
    Begin {
        request_id: u64,
        result: Result<PathListRoot, String>,
    },
    Chunk {
        request_id: u64,
        entries: Arc<[PathEntry]>,
        done: bool,
        truncated: bool,
    },
    Git {
        request_id: u64,
        marks: Vec<(String, GitMark)>,
    },
    Close,
}

pub(crate) struct HostPathListUpdate {
    pub(crate) host: HostId,
    pub(crate) update: PathPickerUpdate,
}

pub(crate) struct AttachmentPreviewRequest {
    pub(crate) image: Arc<Image>,
}

pub(crate) struct SshPromptRequest {
    pub(crate) host: HostId,
    pub(crate) label: String,
    pub(crate) prompt: AskpassPrompt,
    pub(crate) reply: async_channel::Sender<AskpassReply>,
}

const AUTH_DECLINED_REASON: &str = "Authentication was cancelled.\nPick Reconnect when you are \
                                    ready to sign in again.";

fn ssh_destination_label(endpoint: &zz_daemon_client::SshEndpoint) -> String {
    let mut label = String::new();
    if let Some(user) = &endpoint.user {
        label.push_str(user);
        label.push('@');
    }
    if endpoint.host.contains(':') {
        label.push('[');
        label.push_str(&endpoint.host);
        label.push(']');
    } else {
        label.push_str(&endpoint.host);
    }
    if let Some(port) = endpoint.port {
        label.push(':');
        label.push_str(&port.to_string());
    }
    label
}

const fn ala_image_format(format: PastedImageFormat) -> ala::ImageFormat {
    match format {
        PastedImageFormat::Png => ala::ImageFormat::Png,
        PastedImageFormat::Jpeg => ala::ImageFormat::Jpeg,
        PastedImageFormat::Gif => ala::ImageFormat::Gif,
        PastedImageFormat::Webp => ala::ImageFormat::Webp,
    }
}

struct KittyCachedImage {
    generation: u64,
    image: Arc<RenderImage>,
}

#[derive(Default)]
pub(crate) struct KittyImageCache {
    images: HashMap<u32, KittyCachedImage>,
    retired: Vec<Arc<RenderImage>>,
    revision: u64,
    view_count: usize,
}

impl KittyImageCache {
    pub(crate) fn image(&self, image_id: u32, generation: u64) -> Option<Arc<RenderImage>> {
        self.images
            .get(&image_id)
            .filter(|cached| cached.generation == generation)
            .map(|cached| Arc::clone(&cached.image))
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn retain_view(&mut self) {
        self.view_count += 1;
    }

    pub(crate) fn release_view(&mut self) -> Vec<Arc<RenderImage>> {
        assert!(self.view_count > 0);
        self.view_count -= 1;
        if self.view_count != 0 {
            return Vec::new();
        }
        self.images
            .values()
            .map(|cached| Arc::clone(&cached.image))
            .chain(self.retired.drain(..))
            .collect()
    }

    pub(crate) fn take_retired(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.retired)
    }

    fn contains(&self, image_id: u32, generation: u64) -> bool {
        self.images
            .get(&image_id)
            .is_some_and(|cached| cached.generation == generation)
    }

    fn insert(
        &mut self,
        image_id: u32,
        generation: u64,
        image: Arc<RenderImage>,
        retain_replaced: bool,
    ) {
        if self.contains(image_id, generation) {
            return;
        }
        if let Some(previous) = self
            .images
            .insert(image_id, KittyCachedImage { generation, image })
            && retain_replaced
        {
            self.retired.push(previous.image);
        }
        self.revision = self.revision.wrapping_add(1).max(1);
    }

    fn remove(&mut self, image_id: u32, retain_removed: bool) {
        if let Some(previous) = self.images.remove(&image_id) {
            if retain_removed {
                self.retired.push(previous.image);
            }
            self.revision = self.revision.wrapping_add(1).max(1);
        }
    }

    fn clear(&mut self) {
        if self.images.is_empty() {
            return;
        }
        self.retired
            .extend(self.images.drain().map(|(_, cached)| cached.image));
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

struct KittyImageAssembly {
    width: u32,
    height: u32,
    total_bytes: usize,
    bytes: Vec<u8>,
}

struct ArtifactFetch {
    request: ArtifactRequest,
    begun: Option<ArtifactBegun>,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct ArtifactBegun {
    mime: String,
    total: usize,
    reply: bool,
}

struct PastedImageAssembly {
    format: PastedImageFormat,
    total_bytes: usize,
    bytes: Vec<u8>,
}

#[derive(Clone)]
pub(crate) struct CommandOutputModel {
    pub(crate) pane: PaneId,
    pub(crate) retained: Arc<RwLock<RetainedTerminalViewport>>,
}

pub(crate) struct ClientNotification {
    pub(crate) kind: ClientMessageKind,
    pub(crate) text: String,
    pub(crate) duration_ms: Option<u32>,
    pub(crate) message_id: Option<u64>,
    pub(crate) key: Option<String>,
}

pub(crate) struct ClientNotificationCleared {
    pub(crate) message_id: u64,
}

pub(crate) struct InitialConnectionFinished;

pub(crate) struct AgentStateChanged;

pub(crate) struct CommandPrinted {
    pub(crate) request_id: u64,
    pub(crate) output: String,
}

pub(crate) struct AgentCatalogReply(pub(crate) String);

pub(crate) struct KeyTableChanged;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StaleDaemonInfo {
    Protocol { daemon: Option<u16> },
    Revision { daemon: u32, minimum: u32 },
}

impl StaleDaemonInfo {
    const fn from_error(error: &DaemonError) -> Option<Self> {
        match *error {
            DaemonError::IncompatibleDaemon {
                daemon: Some(daemon),
                client,
            } if daemon > client => None,
            DaemonError::IncompatibleDaemon { daemon, .. } => Some(Self::Protocol { daemon }),
            DaemonError::DaemonTooOld { daemon, minimum } => {
                Some(Self::Revision { daemon, minimum })
            }
            _ => None,
        }
    }

    fn restarted_notice(self) -> String {
        match self {
            Self::Protocol {
                daemon: Some(daemon),
            } => format!("restarted stale zz daemon (was v{daemon})"),
            Self::Protocol { daemon: None } => {
                "restarted stale zz daemon (previous version unknown)".to_owned()
            }
            Self::Revision { daemon, .. } => {
                format!("restarted stale zz daemon (was revision {daemon})")
            }
        }
    }
}

const DAEMON_EXIT_DEADLINE: Duration = Duration::from_secs(5);
const DAEMON_EXIT_POLL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalDaemonStop {
    Signal,
    KillServer,
}

impl LocalDaemonStop {
    fn stop(self, socket_path: &Path) -> Result<(), String> {
        if self == Self::KillServer {
            let killed =
                zz_daemon_client::CommandClient::connect(socket_path).and_then(|mut client| {
                    client.execute(CommandInvocation::new("kill-server", [] as [&str; 0]))
                });
            if let Err(DaemonError::Server(error)) = killed {
                return Err(error.tmux_message());
            }
            let deadline = Instant::now() + DAEMON_EXIT_DEADLINE;
            while local_daemon_answers(socket_path) {
                if Instant::now() >= deadline {
                    break;
                }
                thread::sleep(DAEMON_EXIT_POLL);
            }
            if !local_daemon_answers(socket_path) {
                return Ok(());
            }
        }
        terminate_incompatible_daemon(socket_path)
            .map(drop)
            .map_err(|error| error.to_string())
    }
}

fn local_daemon_answers(socket_path: &Path) -> bool {
    use zz_daemon_client::transport::{LocalTransport, Transport as _};

    LocalTransport::connect(socket_path).is_ok()
}

fn client_too_old_text(state: &HostState) -> Option<String> {
    match *state {
        HostState::ClientTooOld { client, minimum } => Some(format!(
            "This zz is too old for the running daemon. It is revision {client}, and the daemon \
             needs revision {minimum} or newer. Update zz."
        )),
        HostState::Incompatible {
            local,
            remote: Some(remote),
        } if remote > local => Some(format!(
            "This zz is too old for the running daemon. It speaks protocol v{local}, and the \
             daemon speaks v{remote}. Update zz."
        )),
        _ => None,
    }
}

fn local_connect_error_text(error: &DaemonError) -> String {
    client_too_old_text(&connect_error_state(error)).unwrap_or_else(|| error.to_string())
}

#[cfg(test)]
struct FakeConnectedHost {
    hello: ServerHello,
    attached_session: std::cell::Cell<Option<SessionId>>,
    attached_default: std::cell::Cell<bool>,
    commands: std::cell::RefCell<Vec<CommandInvocation>>,
    history_requests: std::cell::RefCell<Vec<(PaneId, u32, u32)>>,
    next_request_id: std::cell::Cell<u64>,
}

#[cfg(test)]
impl FakeConnectedHost {
    fn new(hello: ServerHello) -> Self {
        Self {
            hello,
            attached_session: std::cell::Cell::new(None),
            attached_default: std::cell::Cell::new(false),
            commands: std::cell::RefCell::new(Vec::new()),
            history_requests: std::cell::RefCell::new(Vec::new()),
            next_request_id: std::cell::Cell::new(1),
        }
    }

    #[allow(
        clippy::unnecessary_wraps,
        reason = "the fake mirrors the connected client's fallible signature"
    )]
    fn attach(&self, session: Option<SessionId>) -> Result<(), DaemonError> {
        match session {
            Some(session) => self.attached_session.set(Some(session)),
            None => self.attached_default.set(true),
        }
        Ok(())
    }

    #[allow(
        clippy::unnecessary_wraps,
        reason = "the fake mirrors the connected client's fallible signature"
    )]
    fn execute(&self, command: CommandInvocation) -> Result<u64, DaemonError> {
        self.commands.borrow_mut().push(command);
        let request_id = self.next_request_id.get();
        self.next_request_id.set(request_id.saturating_add(1));
        Ok(request_id)
    }

    fn request_history(&self, pane: PaneId, start: u32, count: u32) {
        self.history_requests
            .borrow_mut()
            .push((pane, start, count));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReconnectAttachState {
    RememberedSession,
    DefaultSession,
}

struct HostConnection {
    client: Option<Arc<InteractiveClient>>,
    #[cfg(test)]
    fake_client: Option<Arc<FakeConnectedHost>>,
    reader_thread: Option<thread::JoinHandle<()>>,
    resync_pending: bool,
    full_requests_pending: BTreeSet<PaneId>,
    history: HistoryPacer,
    snapshot: Option<Arc<MuxSnapshot>>,
    appearance: Option<(TerminalAppearance, AppearanceProvenance)>,
    state: HostState,
    route: Arc<RwLock<Option<HostId>>>,
    last_attached_session: Option<SessionId>,
    reconnect_generation: u64,
    reconnect_attempt_in_flight: Option<u32>,
    reconnect_attach: Option<ReconnectAttachState>,
    in_flight_commands: RwLock<VecDeque<(u64, String)>>,
    watched_commands: RwLock<VecDeque<(u64, bool)>>,
    answered_commands: RwLock<BTreeSet<u64>>,
    ssh_auth_declined: bool,
    asked_to_sign_in: bool,
    color_scheme_sent: Option<TerminalColorScheme>,
    background_core: ClientCore,
    providers: Option<zz_protocol::ProvidersReport>,
    providers_requested: bool,
    agent_accounts: Option<Vec<zz_protocol::AgentAccount>>,
    agent_quotas: Vec<zz_protocol::AgentQuota>,
    agent_logins: Vec<zz_protocol::AgentLogin>,
    opened_logins: BTreeSet<u64>,
    platform: zz_protocol::HostPlatform,
}

impl HostConnection {
    fn disconnected(host: HostId) -> Self {
        Self::disconnected_with_route(Arc::new(RwLock::new(Some(host))))
    }

    fn disconnected_with_route(route: Arc<RwLock<Option<HostId>>>) -> Self {
        Self {
            client: None,
            #[cfg(test)]
            fake_client: None,
            reader_thread: None,
            resync_pending: false,
            full_requests_pending: BTreeSet::new(),
            history: HistoryPacer::default(),
            snapshot: None,
            appearance: None,
            state: HostState::Disconnected,
            route,
            last_attached_session: None,
            reconnect_generation: 0,
            reconnect_attempt_in_flight: None,
            reconnect_attach: None,
            in_flight_commands: RwLock::new(VecDeque::new()),
            watched_commands: RwLock::new(VecDeque::new()),
            answered_commands: RwLock::new(BTreeSet::new()),
            ssh_auth_declined: false,
            asked_to_sign_in: false,
            color_scheme_sent: None,
            background_core: ClientCore::new(),
            providers: None,
            providers_requested: false,
            agent_accounts: None,
            agent_quotas: Vec::new(),
            agent_logins: Vec::new(),
            opened_logins: BTreeSet::new(),
            platform: zz_protocol::HostPlatform::Unknown,
        }
    }

    fn connected(
        client: Arc<InteractiveClient>,
        host: HostId,
        cx: &mut Context<MuxClient>,
    ) -> Result<Self, io::Error> {
        Self::connected_with_route(client, Arc::new(RwLock::new(Some(host))), cx)
    }

    fn connected_with_route(
        client: Arc<InteractiveClient>,
        route: Arc<RwLock<Option<HostId>>>,
        cx: &mut Context<MuxClient>,
    ) -> Result<Self, io::Error> {
        let appearance = Some((
            client.server_hello().appearance.clone(),
            client.server_hello().appearance_provenance.clone(),
        ));
        let platform = client.server_hello().platform;
        let mut background_core = ClientCore::new();
        background_core.adopt_hello(client.server_hello().clone());
        let receiver = Arc::clone(&client);
        let (messages, incoming) = decoded_message_channel();
        let reader_thread = thread::Builder::new()
            .name("zz-mux-reader".to_owned())
            .spawn(move || {
                while let Ok(message) = receiver.recv() {
                    if messages.send_blocking(message).is_err() {
                        break;
                    }
                }
            })?;
        let message_route = Arc::clone(&route);
        cx.spawn(async move |this, cx| {
            while let Ok(message) = incoming.recv().await {
                let started = Instant::now();
                let batch = this.update(cx, |client, cx| {
                    drain_decoded_batch(message, &incoming, started, |message| {
                        let Some(host) = *message_route.read() else {
                            return false;
                        };
                        client.handle_message(host, message, cx);
                        true
                    })
                });
                match batch {
                    Ok(Some(true)) => cx.background_executor().timer(Duration::ZERO).await,
                    Ok(Some(false)) => {}
                    _ => break,
                }
            }
            let _ = this.update(cx, |client, cx| {
                if let Some(host) = *message_route.read() {
                    client.handle_host_disconnected(host, cx);
                }
            });
        })
        .detach();
        Ok(Self {
            client: Some(client),
            #[cfg(test)]
            fake_client: None,
            reader_thread: Some(reader_thread),
            resync_pending: false,
            full_requests_pending: BTreeSet::new(),
            history: HistoryPacer::default(),
            snapshot: None,
            appearance,
            state: HostState::Connected,
            route,
            last_attached_session: None,
            reconnect_generation: 0,
            reconnect_attempt_in_flight: None,
            reconnect_attach: None,
            in_flight_commands: RwLock::new(VecDeque::new()),
            watched_commands: RwLock::new(VecDeque::new()),
            answered_commands: RwLock::new(BTreeSet::new()),
            ssh_auth_declined: false,
            asked_to_sign_in: false,
            color_scheme_sent: None,
            background_core,
            providers: None,
            providers_requested: false,
            agent_accounts: None,
            agent_quotas: Vec::new(),
            agent_logins: Vec::new(),
            opened_logins: BTreeSet::new(),
            platform,
        })
    }

    fn track_command(&self, request_id: u64, name: String) {
        let mut in_flight = self.in_flight_commands.write();
        while in_flight.len() >= MAX_TRACKED_COMMANDS {
            in_flight.pop_front();
        }
        in_flight.push_back((request_id, name));
    }

    fn take_command(&self, request_id: u64) -> Option<String> {
        let mut in_flight = self.in_flight_commands.write();
        let index = in_flight.iter().position(|(id, _)| *id == request_id)?;
        in_flight.remove(index).map(|(_, name)| name)
    }

    fn watch_command(&self, request_id: u64) {
        let mut watched = self.watched_commands.write();
        while watched.len() >= MAX_TRACKED_COMMANDS {
            watched.pop_front();
        }
        watched.push_back((request_id, false));
    }

    fn answer_command(&self, request_id: u64) {
        let mut answered = self.answered_commands.write();
        while answered.len() >= MAX_TRACKED_COMMANDS {
            answered.pop_first();
        }
        answered.insert(request_id);
    }

    fn take_answered(&self, request_id: u64) -> bool {
        self.answered_commands.write().remove(&request_id)
    }

    fn settle_command(&self, request_id: u64) -> bool {
        let mut watched = self.watched_commands.write();
        let Some((_, settled)) = watched.iter_mut().find(|(id, _)| *id == request_id) else {
            return false;
        };
        *settled = true;
        true
    }

    fn reroute(&self, host: HostId) {
        *self.route.write() = Some(host);
    }

    fn clear_route(&mut self) {
        self.bump_reconnect_generation();
        self.reconnect_attempt_in_flight = None;
        self.reconnect_attach = None;
        *self.route.write() = None;
    }

    fn bump_reconnect_generation(&mut self) -> u64 {
        self.reconnect_generation = self.reconnect_generation.wrapping_add(1).max(1);
        self.reconnect_generation
    }

    fn is_connected(&self) -> bool {
        let has_client = self.client.is_some();
        #[cfg(test)]
        let has_client = has_client || self.fake_client.is_some();
        debug_assert_eq!(self.state == HostState::Connected, has_client);
        debug_assert_eq!(self.reader_thread.is_some(), self.client.is_some());
        has_client
    }

    fn reconnect_active(&self) -> bool {
        matches!(self.state, HostState::Reconnecting { .. })
            || self.reconnect_attempt_in_flight.is_some()
            || self.reconnect_attach.is_some()
    }

    fn current_hello(&self) -> Option<ServerHello> {
        let hello = self
            .client
            .as_ref()
            .map(|client| client.server_hello().clone());
        #[cfg(test)]
        let hello = hello.or_else(|| self.fake_client.as_ref().map(|client| client.hello.clone()));
        let mut hello = hello?;
        if let Some((appearance, provenance)) = &self.appearance {
            hello.appearance.clone_from(appearance);
            hello.appearance_provenance.clone_from(provenance);
        }
        Some(hello)
    }

    #[cfg(feature = "agent-pane")]
    pub(crate) fn client_instance_id(&self) -> Option<zz_protocol::ClientInstanceId> {
        self.current_hello().map(|hello| hello.client_instance_id)
    }

    fn daemon_revision(&self) -> Option<u32> {
        let revision = self
            .client
            .as_ref()
            .map(|client| client.server_hello().revision);
        #[cfg(test)]
        let revision = revision.or_else(|| {
            self.fake_client
                .as_ref()
                .map(|client| client.hello.revision)
        });
        revision
    }
}

pub(crate) struct TerminalSignal;

pub struct MuxClient {
    core: ClientCore,
    registry: HostRegistry,
    connections: HashMap<HostId, HostConnection>,
    attached_host: HostId,
    color_scheme: TerminalColorScheme,
    appearance: Arc<TerminalAppearance>,
    terminal_font_size_offset_points: f32,
    #[cfg(test)]
    input_sink: Option<std::rc::Rc<std::cell::RefCell<Vec<InputMessage>>>>,
    #[cfg(all(test, feature = "agent-pane"))]
    agent_sink: Option<std::rc::Rc<std::cell::RefCell<Vec<(PaneId, AgentRequest)>>>>,
    #[cfg(all(test, feature = "agent-pane"))]
    agent_client_instance_id: Option<zz_protocol::ClientInstanceId>,
    attached_snapshot_pending: bool,
    desired_client_window_focus: Option<bool>,
    client_focus_attach_ready: bool,
    client_focus_attach_pending: bool,
    client_focus_attach_recoverable: bool,
    sent_client_window_focus: Option<bool>,
    viewports: BTreeMap<PaneId, Arc<RwLock<RetainedTerminalViewport>>>,
    terminal_signals: BTreeMap<PaneId, WeakEntity<TerminalSignal>>,
    kitty_images: BTreeMap<PaneId, Arc<RwLock<KittyImageCache>>>,
    kitty_image_assemblies: BTreeMap<(PaneId, u32, u64), KittyImageAssembly>,
    browser_commands: BTreeMap<PaneId, Vec<BrowserCommand>>,
    #[cfg(feature = "agent-pane")]
    agent_cursors: BTreeMap<PaneId, u64>,
    #[cfg(feature = "agent-pane")]
    agent_replays_pending: BTreeSet<PaneId>,
    #[cfg(feature = "agent-pane")]
    agent_events: AgentEvents,
    #[cfg(feature = "agent-pane")]
    watched_agents: BTreeSet<PaneId>,
    screenshot_requests: Vec<(PaneId, u64, String)>,
    simulator_commands: Vec<(PaneId, zz_protocol::SimulatorCommand)>,
    terminal_commands: BTreeMap<PaneId, Vec<TerminalUiCommand>>,
    command_answers: Vec<(u64, Result<(), String>)>,
    printed_commands: std::cell::RefCell<BTreeSet<u64>>,
    pane_images: BTreeMap<PaneId, PaneImageSnapshots>,
    pasted_image_assemblies: BTreeMap<(PaneId, u32), PastedImageAssembly>,
    pending_pasted_image_previews: BTreeSet<(PaneId, u32)>,
    host_path_lists: BTreeSet<(HostId, u64)>,
    artifact_fetches: BTreeMap<u64, ArtifactFetch>,
    #[cfg(test)]
    path_list_sink: Option<std::rc::Rc<std::cell::RefCell<Vec<ProtocolMessage>>>>,
    pending_commands_revision: u64,
    command_output: Option<CommandOutputModel>,
    command_prompt_revision: u64,
    choose_tree_revision: u64,
    choose_tree_closed_revision: u64,
    choose_buffer_revision: u64,
    display_panes_revision: u64,
    menu_revision: u64,
    confirm_revision: u64,
    next_prefix_cancel_request: u64,
    prefix_cancelled_request: Option<u64>,
    sidebar_focus_revision: u64,
    bell_revision: u64,
    error: Option<Arc<str>>,
    stale_daemon: Option<StaleDaemonInfo>,
    auto_installed: BTreeSet<HostId>,
    host_updates: BTreeMap<String, host_update::HostUpdateSlot>,
    #[cfg(test)]
    install_requests: Option<Vec<HostId>>,
    error_after_next_attach: Option<Arc<str>>,
    shutting_down: bool,
    next_row_revision: u64,
    command_output_diff: TerminalDiffScratch,
    font_names: Option<Arc<[String]>>,
    font_scan: Task<()>,
}

impl MuxClient {
    fn scan_font_names(&mut self, cx: &mut Context<Self>) {
        let text_system = cx.text_system().clone();
        let scan = cx
            .background_executor()
            .spawn(async move { Arc::<[String]>::from(text_system.all_font_names()) });
        self.font_scan = cx.spawn(async move |this, cx| {
            let names = scan.await;
            this.update(cx, |this, _| this.font_names = Some(names))
                .ok();
        });
    }

    fn available_font_names(&mut self, cx: &mut Context<Self>) -> Arc<[String]> {
        let names = self
            .font_names
            .clone()
            .unwrap_or_else(|| cx.text_system().all_font_names().into());
        self.scan_font_names(cx);
        names
    }

    pub(crate) fn host_server_id(&self, host: HostId) -> Option<u64> {
        self.connections
            .get(&host)?
            .client
            .as_ref()
            .map(|client| client.server_hello().server_id)
    }

    pub(crate) fn local_server_id(&self) -> Option<u64> {
        self.connections
            .get(&HostId::LOCAL)?
            .client
            .as_ref()
            .map(|client| client.server_hello().server_id)
    }

    #[cfg(test)]
    pub(crate) fn new(
        client: Result<InteractiveClient, DaemonError>,
        local_socket_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let color_scheme = client.as_ref().map_or(TerminalColorScheme::Dark, |client| {
            client.server_hello().appearance.color_scheme
        });
        Self::new_inner(Some(client), local_socket_path, color_scheme, cx)
    }

    pub fn new_connecting(
        local_socket_path: PathBuf,
        color_scheme: TerminalColorScheme,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = Self::new_inner(None, local_socket_path.clone(), color_scheme, cx);
        let connection = cx.background_executor().spawn(async move {
            crate::connect_interactive_client(&local_socket_path, color_scheme)
        });
        cx.spawn(async move |this, cx| {
            let result = connection.await;
            let _ = this.update(cx, |state, cx| {
                state.finish_initial_connection(result, cx);
            });
        })
        .detach();
        state
    }

    fn new_inner(
        client: Option<Result<InteractiveClient, DaemonError>>,
        local_socket_path: PathBuf,
        color_scheme: TerminalColorScheme,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_global::<crate::config::FleetHosts>(|client, cx| {
            client.reconcile_hosts(cx);
        })
        .detach();
        cx.observe_global::<crate::config::DiscoveredHosts>(|client, cx| {
            client.reconcile_hosts(cx);
        })
        .detach();
        cx.observe_global::<crate::config::DaemonConfigOverrides>(|client, cx| {
            if client.attached_host != HostId::LOCAL {
                client.adopt_core_appearance(cx);
                cx.notify();
            }
        })
        .detach();
        let local_host_policy = crate::profile::profile(cx).local_host;
        let registry = HostRegistry::new(
            local_socket_path,
            &crate::config::known_hosts(cx),
            local_host_policy,
        );
        let has_local = registry.get(HostId::LOCAL).is_some();
        let mut connections: HashMap<HostId, HostConnection> = registry
            .iter()
            .map(|(host, _)| (host, HostConnection::disconnected(host)))
            .collect();
        if !has_local {
            connections.insert(HostId::LOCAL, HostConnection::disconnected(HostId::LOCAL));
        }
        let mut state = Self {
            core: ClientCore::new(),
            registry,
            connections,
            attached_host: HostId::LOCAL,
            color_scheme,
            appearance: Arc::new(TerminalAppearance::default()),
            terminal_font_size_offset_points: 0.0,
            #[cfg(test)]
            input_sink: None,
            #[cfg(all(test, feature = "agent-pane"))]
            agent_sink: None,
            #[cfg(all(test, feature = "agent-pane"))]
            agent_client_instance_id: None,
            attached_snapshot_pending: false,
            desired_client_window_focus: None,
            client_focus_attach_ready: false,
            client_focus_attach_pending: false,
            client_focus_attach_recoverable: false,
            sent_client_window_focus: None,
            viewports: BTreeMap::new(),
            terminal_signals: BTreeMap::new(),
            kitty_images: BTreeMap::new(),
            kitty_image_assemblies: BTreeMap::new(),
            browser_commands: BTreeMap::new(),
            #[cfg(feature = "agent-pane")]
            agent_cursors: BTreeMap::new(),
            #[cfg(feature = "agent-pane")]
            agent_replays_pending: BTreeSet::new(),
            #[cfg(feature = "agent-pane")]
            agent_events: AgentEvents::default(),
            #[cfg(feature = "agent-pane")]
            watched_agents: BTreeSet::new(),
            screenshot_requests: Vec::new(),
            simulator_commands: Vec::new(),
            terminal_commands: BTreeMap::new(),
            command_answers: Vec::new(),
            printed_commands: std::cell::RefCell::default(),
            pane_images: BTreeMap::new(),
            pasted_image_assemblies: BTreeMap::new(),
            pending_pasted_image_previews: BTreeSet::new(),
            host_path_lists: BTreeSet::new(),
            artifact_fetches: BTreeMap::new(),
            #[cfg(test)]
            path_list_sink: None,
            pending_commands_revision: 0,
            command_output: None,
            command_prompt_revision: 0,
            choose_tree_revision: 0,
            choose_tree_closed_revision: 0,
            choose_buffer_revision: 0,
            display_panes_revision: 0,
            menu_revision: 0,
            confirm_revision: 0,
            next_prefix_cancel_request: 0,
            prefix_cancelled_request: None,
            sidebar_focus_revision: 0,
            bell_revision: 0,
            error: None,
            stale_daemon: None,
            auto_installed: BTreeSet::new(),
            host_updates: BTreeMap::new(),
            #[cfg(test)]
            install_requests: Some(Vec::new()),
            error_after_next_attach: None,
            shutting_down: false,
            next_row_revision: 1,
            command_output_diff: TerminalDiffScratch::default(),
            font_names: None,
            font_scan: Task::ready(()),
        };
        state.scan_font_names(cx);
        if has_local {
            match client {
                Some(Ok(client)) => {
                    if let Err(error) = state.install_initial_local_connection(client, cx) {
                        state.error = Some(Arc::from(error));
                        return state;
                    }
                }
                Some(Err(error)) => {
                    state.stale_daemon = StaleDaemonInfo::from_error(&error);
                    state.error = Some(Arc::from(local_connect_error_text(&error)));
                }
                None => {
                    state.connections.get_mut(&HostId::LOCAL).unwrap().state =
                        HostState::Connecting;
                }
            }
        } else if let Some(Ok(client)) = client {
            let _ = client.detach();
        }
        if !has_local
            || state
                .connections
                .get(&HostId::LOCAL)
                .is_some_and(HostConnection::is_connected)
        {
            state.ensure_all_connected(cx);
        }
        state
    }

    fn shown_appearance(&self, cx: &App) -> (TerminalAppearance, AppearanceProvenance) {
        let host = self.core.appearance().cloned().unwrap_or_default();
        if self.attached_host == HostId::LOCAL {
            return (host, self.core.appearance_provenance().clone());
        }
        let local = crate::config::local_terminal_appearance(self.color_scheme, cx);
        let mut appearance = local.appearance;
        let mut colors = [appearance.foreground; 18];
        colors[1] = appearance.background;
        colors[2..].copy_from_slice(&appearance.palette.as_array()[..16]);
        appearance.local_colors = Some(LocalColors::new(
            host.foreground,
            host.background,
            &host.palette,
            colors,
        ));
        (appearance, local.provenance)
    }

    fn ingest_server_hello(&mut self, hello: ServerHello, cx: &mut Context<Self>) {
        self.core.adopt_hello(hello);
        let (mut appearance, provenance) = self.shown_appearance(cx);
        let requested_primary_font = appearance.font_families.first().cloned();
        localize_terminal_font_families(
            &mut appearance,
            &provenance,
            &self.available_font_names(cx),
        );
        self.terminal_font_size_offset_points =
            apply_terminal_font_size_offset(&mut appearance, self.terminal_font_size_offset_points);
        self.appearance = Arc::new(appearance);
        log::info!(
            target: "zz::diagnostics::appearance",
            "resolved appearance hash={} requested_primary_font={requested_primary_font:?} primary_font={:?} fallback_count={} feature_count={} font_size_points={} padding_left={} padding_right={} padding_top={} padding_bottom={} minimum_contrast={} background_opacity={} blink_policy={:?} blink_interval_ms={}",
            self.appearance.stable_hash(),
            self.appearance.font_families.first(),
            self.appearance.font_families.len().saturating_sub(1),
            self.appearance.font_features.len(),
            self.appearance.font_size_points,
            self.appearance.padding_left,
            self.appearance.padding_right,
            self.appearance.padding_top,
            self.appearance.padding_bottom,
            self.appearance.minimum_contrast,
            self.appearance.background_opacity,
            self.appearance.cursor_blink_policy,
            self.appearance.cursor_blink_interval_ms,
        );
        crate::theme::set_terminal_appearance(Arc::clone(&self.appearance), cx);
    }

    #[cfg(test)]
    fn discard_core_effects(&mut self) {
        while self.core.poll_outbound().is_some() {}
        while self.core.poll_event().is_some() {}
    }

    fn attached_connection(&self) -> &HostConnection {
        debug_assert!(self.connections.contains_key(&self.attached_host));
        &self.connections[&self.attached_host]
    }

    fn attached_connection_mut(&mut self) -> &mut HostConnection {
        let attached_host = self.attached_host;
        debug_assert!(self.connections.contains_key(&attached_host));
        self.connections.get_mut(&attached_host).unwrap()
    }

    pub(crate) fn attached_ssh_egress(&self) -> Option<(String, u16)> {
        let entry = self.registry.get(self.attached_host)?;
        let Endpoint::Ssh(endpoint) = &entry.endpoint else {
            return None;
        };
        let port = self.attached_connection().client.as_ref()?.socks_port()?;
        Some((ssh_destination_label(endpoint), port))
    }

    fn request_history_backfill(&mut self, pane: PaneId) {
        self.request_history(pane, None, Instant::now());
    }

    fn defer_history_backfill(
        &mut self,
        pane: PaneId,
        history_mutations: u64,
        cx: &mut Context<Self>,
    ) {
        let host = self.attached_host;
        if !self
            .attached_connection_mut()
            .history
            .defer(pane, history_mutations)
        {
            return;
        }

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(HISTORY_BACKFILL_QUIET).await;
                let rearm = this.update(cx, |client, _cx| {
                    if client.attached_host != host {
                        return false;
                    }
                    let mutations = client
                        .viewports
                        .get(&pane)
                        .map(|retained| retained.read().history_mutations);
                    let Some(connection) = client.connections.get_mut(&host) else {
                        return false;
                    };
                    match connection.history.resume_deferred(pane, mutations) {
                        DeferredBackfill::Request => {
                            client.request_history_backfill(pane);
                            false
                        }
                        DeferredBackfill::Wait => true,
                        DeferredBackfill::Done => false,
                    }
                });
                if !matches!(rearm, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn request_history_prefetch(&mut self, pane: PaneId, target_offset: u32) {
        self.request_history(pane, Some(target_offset), Instant::now());
    }

    fn request_history(&mut self, pane: PaneId, prefetch_target: Option<u32>, now: Instant) {
        let trickle_budget = self.core.history_trickle_budget();
        let retained = self.viewports.get(&pane).cloned();
        let retained = retained.as_ref().map(|retained| retained.read());
        let connection = self.attached_connection_mut();
        let Some((start, count)) = connection.history.request(
            pane,
            prefetch_target,
            trickle_budget,
            retained.as_deref(),
            now,
        ) else {
            return;
        };
        drop(retained);
        if let Some(client) = &connection.client {
            if let Err(error) = client.request_history(pane, start, count) {
                connection.history.forget(pane);
                log::warn!("failed to request terminal history for {pane}: {error}");
            }
            return;
        }
        #[cfg(test)]
        if let Some(client) = &connection.fake_client {
            client.request_history(pane, start, count);
            return;
        }
        connection.history.forget(pane);
    }

    pub const fn attached_host(&self) -> HostId {
        self.attached_host
    }

    #[cfg(feature = "agent-pane")]
    pub(crate) fn client_instance_id(&self) -> Option<zz_protocol::ClientInstanceId> {
        #[cfg(test)]
        if self.agent_client_instance_id.is_some() {
            return self.agent_client_instance_id;
        }
        self.attached_connection().client_instance_id()
    }

    #[must_use]
    pub fn snapshot(&self) -> Arc<MuxSnapshot> {
        Arc::clone(self.core.snapshot())
    }

    #[must_use]
    pub(crate) const fn layout_generation(&self) -> u64 {
        self.core.layout_generation()
    }

    pub(crate) fn agent_attention_status(
        &self,
        pane: PaneId,
    ) -> Option<zz_client::AgentAttentionStatus> {
        self.core
            .agent_state(pane)
            .map(zz_client::agent_attention_status)
    }

    pub(crate) fn claims_prefix_input(&self, input: &zz_terminal::KeyInput) -> bool {
        self.attached_connection().client.is_some() && self.core.claims_prefix_input(input)
    }

    #[must_use]
    pub(crate) fn canonical_prefix(&self) -> Option<String> {
        self.attached_connection().client.as_ref()?;
        self.core.canonical_prefix()
    }

    #[must_use]
    pub(crate) fn command_aliases(&self) -> String {
        self.core
            .mux_options()
            .get(MuxOptionKey::CommandAlias)
            .map(|option| option.value.clone())
            .unwrap_or_default()
    }

    #[must_use]
    pub(crate) fn focus_follows_mouse(&self) -> bool {
        self.core
            .mux_options()
            .get(MuxOptionKey::FocusFollowsMouse)
            .is_some_and(|option| option.value == "on")
    }

    fn write_clipboard(
        &self,
        pane: PaneId,
        producer: ClipboardProducer,
        target: ClipboardTarget,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if text.is_empty() {
            return;
        }
        let item = ClipboardItem::new_string(text);
        let empty_field = producer == ClipboardProducer::Server;
        let lands_in_clipboard = self.core.clipboard_write_lands(producer, target);
        if empty_field
            && lands_in_clipboard
            && let Some(retained) = self.viewports.get(&pane)
        {
            let mut retained = retained.write();
            retained.copy_generation = retained.copy_generation.wrapping_add(1);
        }
        match target {
            ClipboardTarget::Clipboard => cx.write_to_clipboard(item),
            ClipboardTarget::Primary => {
                if lands_in_clipboard {
                    cx.write_to_clipboard(item.clone());
                }
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                cx.write_to_primary(item);
            }
        }
    }

    #[must_use]
    pub(crate) const fn prefix_armed(&self) -> bool {
        self.core.prefix_armed()
    }

    #[must_use]
    pub(crate) fn key_table(&self) -> Option<(&str, bool)> {
        self.attached_connection().client.as_ref()?;
        self.core.key_table()
    }

    #[must_use]
    pub(crate) fn key_tables(&self) -> &[KeyTableSnapshot] {
        if self.attached_connection().client.is_none() {
            return &[];
        }
        self.core.key_tables()
    }

    #[must_use]
    pub(crate) const fn prefix_cancelled_request(&self) -> Option<u64> {
        self.prefix_cancelled_request
    }

    #[must_use]
    pub(crate) fn prefix_bindings(&self) -> &[KeyBindingSnapshot] {
        if self.attached_connection().client.is_none() {
            return &[];
        }
        self.core.prefix_bindings()
    }

    #[must_use]
    pub(crate) fn local_prefix_bindings(&self) -> &[KeyBindingSnapshot] {
        if self.attached_host == HostId::LOCAL {
            return self.prefix_bindings();
        }
        self.connections
            .get(&HostId::LOCAL)
            .filter(|local| local.client.is_some())
            .map_or(&[], |local| local.background_core.prefix_bindings())
    }

    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.attached_connection().is_connected()
    }

    #[must_use]
    pub(crate) fn local_is_connected(&self) -> bool {
        self.connections
            .get(&HostId::LOCAL)
            .is_some_and(HostConnection::is_connected)
    }

    #[cfg(test)]
    pub(crate) fn record_path_lists_for_test(
        &mut self,
    ) -> std::rc::Rc<std::cell::RefCell<Vec<ProtocolMessage>>> {
        let sink = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        self.path_list_sink = Some(std::rc::Rc::clone(&sink));
        sink
    }

    #[cfg(test)]
    pub(crate) fn record_input_for_test(
        &mut self,
    ) -> std::rc::Rc<std::cell::RefCell<Vec<InputMessage>>> {
        let sink = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        self.input_sink = Some(std::rc::Rc::clone(&sink));
        sink
    }

    #[cfg(all(test, feature = "agent-pane"))]
    pub(crate) fn record_agent_requests_for_test(
        &mut self,
    ) -> std::rc::Rc<std::cell::RefCell<Vec<(PaneId, AgentRequest)>>> {
        let sink = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        self.agent_sink = Some(std::rc::Rc::clone(&sink));
        sink
    }

    #[cfg(all(test, feature = "agent-pane"))]
    pub(crate) fn set_agent_client_instance_id_for_test(
        &mut self,
        client_instance_id: zz_protocol::ClientInstanceId,
    ) {
        self.agent_client_instance_id = Some(client_instance_id);
    }

    #[cfg(test)]
    fn seed_core(&mut self, payload: EventPayload) {
        self.core.handle_message(ProtocolMessage::Event(Event {
            sequence: 0,
            payload,
        }));
        self.discard_core_effects();
    }

    #[cfg(test)]
    pub(crate) fn set_prefix_armed_for_test(&mut self, armed: bool, cx: &mut Context<Self>) {
        self.seed_core(EventPayload::PrefixArmed { armed });
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn acknowledge_prefix_cancel_for_test(
        &mut self,
        request_id: u64,
        cx: &mut Context<Self>,
    ) {
        self.seed_core(EventPayload::PrefixCancelled { request_id });
        self.prefix_cancelled_request = self.prefix_cancelled_request.max(Some(request_id));
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn reset_session_state_for_test(&mut self, cx: &mut Context<Self>) {
        self.reset_session_state(cx);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn attach_snapshot_for_test(
        &mut self,
        session: SessionId,
        snapshot: MuxSnapshot,
        cx: &mut Context<Self>,
    ) {
        self.core.handle_message(ProtocolMessage::Attached {
            session,
            snapshot,
            read_only: false,
            client_flags: String::new(),
        });
        self.discard_core_effects();
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn handle_message_for_test(
        &mut self,
        message: ProtocolMessage,
        cx: &mut Context<Self>,
    ) {
        self.handle_message(HostId::LOCAL, message, cx);
    }

    #[cfg(test)]
    #[allow(
        clippy::arc_with_non_send_sync,
        reason = "the fake host lives on the test thread, like the connection it stands in for"
    )]
    pub(crate) fn connect_for_test(&mut self) {
        let host = self.attached_host;
        let connection = self.connections.get_mut(&host).expect("host connection");
        connection.client = None;
        connection.fake_client = Some(Arc::new(FakeConnectedHost::new(ServerHello {
            protocol_version: PROTOCOL_VERSION,
            server_id: 1,
            client_id: zz_protocol::ClientId(1),
            client_instance_id: zz_protocol::ClientInstanceId(1),
            capabilities: Vec::new(),
            appearance: TerminalAppearance::default(),
            appearance_provenance: AppearanceProvenance::default(),
            mux_options: zz_protocol::MuxOptions::default(),
            status: StatusLine::default(),
            key_tables: Vec::new(),
            clock: 0,
            platform: zz_protocol::HostPlatform::default(),
            revision: zz_protocol::REVISION,
        })));
        connection.reader_thread = None;
        connection.state = HostState::Connected;
    }

    #[cfg(test)]
    pub(crate) fn executed_for_test(&self) -> Vec<CommandInvocation> {
        self.connections
            .get(&self.attached_host)
            .and_then(|connection| connection.fake_client.as_ref())
            .map(|fake| fake.commands.borrow().clone())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn attach_host_for_test(&mut self, name: &str, cx: &mut Context<Self>) {
        let (host, _) = self
            .registry
            .get_by_name(name)
            .expect("a fleet host registered for the test");
        self.attached_host = host;
        self.core.clear_attachment();
        cx.notify();
    }

    #[must_use]
    pub(crate) fn appearance(&self) -> Arc<TerminalAppearance> {
        Arc::clone(&self.appearance)
    }

    pub(crate) fn adjust_terminal_font_size(
        &mut self,
        adjustment: TerminalFontSizeAdjustment,
        cx: &mut Context<Self>,
    ) {
        let previous = self.appearance.font_size_points;
        let next = adjusted_terminal_font_size(previous, adjustment);
        let applied_delta = next - previous;
        if applied_delta.abs() < f32::EPSILON {
            return;
        }

        self.terminal_font_size_offset_points += applied_delta;
        let mut appearance = self.appearance.as_ref().clone();
        appearance.font_size_points = next;
        self.appearance = Arc::new(appearance);
        log::info!(
            target: "zz::diagnostics::appearance",
            "terminal font size adjustment={adjustment:?} previous_points={previous} points={next} offset_points={}",
            self.terminal_font_size_offset_points,
        );
        cx.notify();
    }

    #[must_use]
    pub(crate) fn viewport(&self, pane: PaneId) -> Option<Arc<RwLock<RetainedTerminalViewport>>> {
        self.viewports.get(&pane).cloned()
    }

    #[must_use]
    pub(crate) fn terminal_signal(
        &mut self,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) -> Entity<TerminalSignal> {
        if let Some(signal) = self
            .terminal_signals
            .get(&pane)
            .and_then(WeakEntity::upgrade)
        {
            return signal;
        }
        self.terminal_signals
            .retain(|_, signal| signal.upgrade().is_some());
        let signal = cx.new(|_| TerminalSignal);
        self.terminal_signals.insert(pane, signal.downgrade());
        signal
    }

    fn signal_terminal(&self, pane: PaneId, cx: &mut Context<Self>) {
        if let Some(signal) = self
            .terminal_signals
            .get(&pane)
            .and_then(WeakEntity::upgrade)
        {
            signal.update(cx, |_, cx| cx.notify());
        }
    }

    pub(crate) fn kitty_images(&self, pane: PaneId) -> Option<Arc<RwLock<KittyImageCache>>> {
        self.kitty_images.get(&pane).cloned()
    }

    #[must_use]
    pub(crate) fn command_output(&self) -> Option<CommandOutputModel> {
        self.command_output.clone()
    }

    #[must_use]
    pub(crate) fn error(&self) -> Option<Arc<str>> {
        self.error.clone()
    }

    #[must_use]
    pub(crate) fn stale_daemon(&self) -> Option<StaleDaemonInfo> {
        self.stale_daemon
    }

    #[must_use]
    pub(crate) fn daemon_revision(&self, host: HostId) -> Option<u32> {
        self.connections.get(&host)?.daemon_revision()
    }

    #[must_use]
    pub(crate) fn daemon_behind(&self, host: HostId) -> Option<DaemonBehind> {
        DaemonBehind::compare(self.daemon_revision(host)?, zz_protocol::REVISION)
    }

    #[must_use]
    pub(crate) fn host_session_count(&self, host: HostId) -> Option<usize> {
        if !self.connections.get(&host)?.is_connected() {
            return None;
        }
        self.host_snapshot(host)
            .map(|snapshot| snapshot.sessions.len())
    }

    #[must_use]
    pub(crate) fn attached_client_too_old(&self) -> bool {
        client_too_old_text(&self.attached_connection().state).is_some()
    }

    pub(crate) fn emit_notification(
        kind: ClientMessageKind,
        text: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        Self::emit_keyed_notification(kind, text, None, cx);
    }

    fn emit_keyed_notification(
        kind: ClientMessageKind,
        text: impl Into<String>,
        key: Option<String>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(ClientNotification {
            kind,
            text: text.into(),
            duration_ms: None,
            message_id: None,
            key,
        });
    }

    #[must_use]
    pub const fn sidebar_focus_revision(&self) -> u64 {
        self.sidebar_focus_revision
    }

    #[must_use]
    pub(crate) const fn bell_revision(&self) -> u64 {
        self.bell_revision
    }

    #[must_use]
    pub fn attached_session(&self) -> Option<SessionId> {
        self.core.attached_session()
    }

    #[must_use]
    pub(crate) fn active_pane(&self) -> Option<PaneId> {
        let attached = self.attached_session()?;
        let snapshot = self.snapshot();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| session.id == attached)?;
        let focused_window = snapshot.focused_window_for(session);
        session
            .windows
            .iter()
            .find(|window| window.id == focused_window)
            .map(|window| window.active_pane)
    }

    #[must_use]
    pub(crate) fn modal_capture(&self) -> Option<PaneId> {
        self.core.modal_capture()
    }

    pub(crate) fn attached_is_local(&self) -> bool {
        self.attached_host == HostId::LOCAL
    }

    pub fn set_color_scheme(&mut self, color_scheme: TerminalColorScheme, cx: &mut Context<Self>) {
        let changed = self.color_scheme != color_scheme;
        self.color_scheme = color_scheme;
        self.report_color_scheme();
        if changed && self.attached_host != HostId::LOCAL {
            self.adopt_core_appearance(cx);
            cx.notify();
        }
    }

    fn report_color_scheme(&mut self) {
        let color_scheme = self.color_scheme;
        let connection = self.attached_connection_mut();
        if connection.color_scheme_sent == Some(color_scheme) {
            return;
        }
        let Some(sent) = connection
            .client
            .as_ref()
            .map(|client| client.set_color_scheme(color_scheme))
        else {
            return;
        };
        match sent {
            Ok(()) => connection.color_scheme_sent = Some(color_scheme),
            Err(error) => log::warn!("failed to report the color scheme: {error}"),
        }
    }

    pub(crate) fn take_browser_commands(&mut self, pane: PaneId) -> Vec<BrowserCommand> {
        self.browser_commands.remove(&pane).unwrap_or_default()
    }

    #[must_use]
    pub fn simulator(&self, pane: PaneId) -> Option<SimulatorDescriptor> {
        self.core.simulator(pane).cloned()
    }

    #[must_use]
    pub fn agent_parent(&self, host: HostId, pane: PaneId) -> Option<PaneId> {
        self.connections
            .get(&host)?
            .background_core
            .agent_parent(pane)
    }

    pub(crate) fn take_terminal_commands(&mut self, pane: PaneId) -> Vec<TerminalUiCommand> {
        self.terminal_commands.remove(&pane).unwrap_or_default()
    }

    pub(crate) fn take_screenshot_requests(&mut self) -> Vec<(PaneId, u64, String)> {
        std::mem::take(&mut self.screenshot_requests)
    }

    #[must_use]
    pub(crate) fn has_gui_requests(&self) -> bool {
        !self.screenshot_requests.is_empty() || !self.simulator_commands.is_empty()
    }

    pub(crate) fn take_simulator_commands(
        &mut self,
    ) -> Vec<(PaneId, zz_protocol::SimulatorCommand)> {
        std::mem::take(&mut self.simulator_commands)
    }

    pub(crate) fn takes_screenshots(&self) -> bool {
        self.attached_connection()
            .client
            .as_ref()
            .is_some_and(|client| client.daemon_has(zz_protocol::BROWSER_SCREENSHOT_CAPABILITY))
    }

    pub(crate) fn send_screenshot(&self, request_id: u64, png: &[u8]) -> Result<(), String> {
        let Some(client) = &self.attached_connection().client else {
            return Err("the zz window is not connected".to_owned());
        };
        let chunks = png
            .chunks(zz_protocol::MAX_BROWSER_SCREENSHOT_CHUNK_BYTES)
            .collect::<Vec<_>>();
        let last = chunks.len().saturating_sub(1);
        for (index, chunk) in chunks.into_iter().enumerate() {
            let message = zz_protocol::ProtocolMessage::BrowserScreenshot {
                request_id,
                last: index == last,
                png: chunk.to_vec(),
            };
            client
                .send(&message)
                .map_err(|error| format!("could not send the screenshot: {error}"))?;
        }
        Ok(())
    }

    pub(crate) fn respond_to_request(&self, response: GuiResponse) {
        if let Some(client) = &self.attached_connection().client
            && let Err(error) = client.send_gui_response(response)
        {
            log::warn!("failed to answer a daemon GUI request: {error}");
        }
    }

    #[must_use]
    pub(crate) const fn pending_commands_revision(&self) -> u64 {
        self.pending_commands_revision
    }

    pub fn detach(&mut self) {
        self.shutting_down = true;
        if let Some(client) = &self.attached_connection().client {
            let _ = client.detach();
        }
    }

    pub(crate) fn log_diagnostic_snapshot(&self, reason: &str) {
        log::info!(
            target: "zz::diagnostics::mux_state",
            "snapshot reason={reason} connected={} host_state={} appearance_hash={} terminal_font_size_points={} terminal_font_size_offset_points={} attached_session={:?} mux_generation={} sessions={} retained_viewports={} browser_command_panes={} terminal_command_panes={} command_output={} command_prompt={} choose_tree={} choose_buffer={} display_panes={} next_row_revision={} resync_pending={} error={:?}",
            self.attached_connection().client.is_some(),
            self.attached_connection().state.label(),
            self.appearance.stable_hash(),
            self.appearance.font_size_points,
            self.terminal_font_size_offset_points,
            self.core.attached_session(),
            self.core.snapshot().generation,
            self.core.snapshot().sessions.len(),
            self.viewports.len(),
            self.browser_commands.len(),
            self.terminal_commands.len(),
            self.command_output.is_some(),
            self.core.command_prompt().is_some(),
            self.core.choose_tree().is_some(),
            self.core.choose_buffer().is_some(),
            self.core.display_panes().is_some(),
            self.next_row_revision,
            self.attached_connection().resync_pending,
            self.error,
        );
        log::trace!(
            target: "zz::diagnostics::mux_state",
            "snapshot reason={reason} mux={:#?} browser_commands={:#?} terminal_commands={:#?} command_prompt={:#?} choose_tree={:#?} choose_buffer={:#?} display_panes={:#?}",
            self.core.snapshot(),
            self.browser_commands,
            self.terminal_commands,
            self.core.command_prompt(),
            self.core.choose_tree(),
            self.core.choose_buffer(),
            self.core.display_panes(),
        );
        for (pane, retained) in &self.viewports {
            let retained_state = retained.read();
            let viewport = &retained_state.viewport;
            log::info!(
                target: "zz::diagnostics::terminal_state",
                "snapshot reason={reason} pane={pane} retained_strong_count={} generation={} view_generation={} dictionary_generation={} columns={} rows={} cell_count={} cell_bytes={} cell_arc_strong_count={} dictionary_arc_strong_count={} styles={} graphemes={} grapheme_bytes={} overlays={} row_revisions={} row_revision_epoch={} revision_scratch_len={} revision_scratch_capacity={} title={:?} cursor={:?} scrollbar={:?} mode={:?} search={:?} unseen_output={} kitty_keyboard={} mouse_tracking={} status={:?}",
                Arc::strong_count(retained),
                viewport.generation,
                viewport.view_generation,
                viewport.dictionary_generation,
                viewport.columns,
                viewport.rows,
                viewport.cells.len(),
                std::mem::size_of_val(viewport.cells.as_ref()),
                Arc::strong_count(&viewport.cells),
                Arc::strong_count(&viewport.dictionary),
                viewport.dictionary.styles.len(),
                viewport.dictionary.grapheme_offsets.len(),
                viewport.dictionary.grapheme_bytes.len(),
                viewport.overlays.len(),
                retained_state.row_revisions.len(),
                retained_state.row_revision_epoch,
                retained_state.revision_scratch.len(),
                retained_state.revision_scratch.capacity(),
                viewport.title(),
                viewport.cursor,
                viewport.scrollbar,
                viewport.mode,
                viewport.search,
                viewport.unseen_output,
                viewport.kitty_keyboard,
                viewport.mouse_tracking,
                viewport.status,
            );
            log::trace!(
                target: "zz::diagnostics::terminal_state",
                "snapshot reason={reason} pane={pane} viewport={viewport:#?} row_revisions={:#?}",
                retained_state.row_revisions,
            );
        }
        if let Some(output) = &self.command_output {
            let retained = output.retained.read();
            log::trace!(
                target: "zz::diagnostics::terminal_state",
                "snapshot reason={reason} command_output_pane={} viewport={:#?} row_revisions={:#?}",
                output.pane,
                retained.viewport,
                retained.row_revisions,
            );
        }
    }

    fn handle_message(&mut self, host: HostId, message: ProtocolMessage, cx: &mut Context<Self>) {
        let Some(message) = self.route_host_path_list(host, message, cx) else {
            return;
        };
        let message = match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Providers(report),
                ..
            }) => {
                if let Some(connection) = self.connections.get_mut(&host) {
                    connection.providers = Some(report);
                }
                cx.notify();
                return;
            }
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentAccounts(accounts),
                ..
            }) => {
                if let Some(connection) = self.connections.get_mut(&host) {
                    connection.agent_accounts = Some(accounts);
                }
                cx.notify();
                return;
            }
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentQuotas(quotas),
                ..
            }) => {
                if let Some(connection) = self.connections.get_mut(&host) {
                    connection.agent_quotas = quotas;
                }
                cx.notify();
                return;
            }
            ProtocolMessage::Event(Event {
                payload: EventPayload::AgentLogin(login),
                ..
            }) => {
                self.observe_agent_login(host, login, cx);
                return;
            }
            ProtocolMessage::Batch(batch) => {
                match batch.messages() {
                    Ok(messages) => {
                        let attached_before = self.core.attached_session();
                        let active = host == self.attached_host;
                        if active {
                            self.core.begin_event_group();
                        }
                        if let Some(connection) = self.connections.get_mut(&host) {
                            connection.background_core.begin_event_group();
                        }
                        for message in messages {
                            self.handle_message(host, message, cx);
                        }
                        if let Some(connection) = self.connections.get_mut(&host) {
                            connection.background_core.finish_event_group();
                            connection.snapshot =
                                Some(Arc::clone(connection.background_core.snapshot()));
                            while let Some(outbound) = connection.background_core.poll_outbound() {
                                if !active
                                    && matches!(outbound, Outbound::TreeSync)
                                    && let Some(client) = &connection.client
                                {
                                    let _ = client.send(&ProtocolMessage::TreeSync);
                                }
                            }
                            while connection.background_core.poll_event().is_some() {}
                        }
                        if active {
                            self.core.finish_event_group();
                            while let Some(outbound) = self.core.poll_outbound() {
                                match outbound {
                                    Outbound::RequestFull(pane) => self.request_full_viewport(pane),
                                    Outbound::TreeSync => {
                                        if let Some(client) = &self.attached_connection().client {
                                            let _ = client.send(&ProtocolMessage::TreeSync);
                                        }
                                    }
                                }
                            }
                            let mut attaching = true;
                            while let Some(event) = self.core.poll_event() {
                                self.apply_core_event(
                                    host,
                                    event,
                                    attached_before,
                                    &mut attaching,
                                    cx,
                                );
                            }
                        }
                        cx.notify();
                    }
                    Err(error) => {
                        self.error = Some(Arc::from(error.to_string()));
                        cx.notify();
                    }
                }
                return;
            }
            message => message,
        };
        if host == self.attached_host
            && let ProtocolMessage::Event(event) = &message
            && matches!(
                event.payload,
                EventPayload::Snapshot(_)
                    | EventPayload::TreeDelta(_)
                    | EventPayload::ClientView(_)
                    | EventPayload::KeyTablesChanged { .. }
                    | EventPayload::KeyTablesPatched { .. }
                    | EventPayload::AgentParents { .. }
            )
            && let Some(connection) = self.connections.get_mut(&host)
        {
            connection.background_core.handle_message(message.clone());
            while connection.background_core.poll_event().is_some() {}
            while connection.background_core.poll_outbound().is_some() {}
        }
        if host != self.attached_host {
            match message {
                ProtocolMessage::Event(event) => match event.payload {
                    payload @ (EventPayload::Snapshot(_)
                    | EventPayload::TreeDelta(_)
                    | EventPayload::ClientView(_)
                    | EventPayload::KeyTablesChanged { .. }
                    | EventPayload::KeyTablesPatched { .. }
                    | EventPayload::AgentParents { .. }) => {
                        if let Some(connection) = self.connections.get_mut(&host) {
                            connection
                                .background_core
                                .handle_message(ProtocolMessage::Event(Event {
                                    sequence: event.sequence,
                                    payload,
                                }));
                            connection.snapshot =
                                Some(Arc::clone(connection.background_core.snapshot()));
                            while let Some(outbound) = connection.background_core.poll_outbound() {
                                if matches!(outbound, Outbound::TreeSync)
                                    && let Some(client) = &connection.client
                                {
                                    let _ = client.send(&ProtocolMessage::TreeSync);
                                }
                            }
                            while connection.background_core.poll_event().is_some() {}
                            cx.notify();
                        }
                    }
                    EventPayload::Bell { pane } => {
                        self.record_bell(host, pane);
                        cx.notify();
                    }
                    EventPayload::AppearanceChanged {
                        appearance,
                        provenance,
                    } => {
                        if let Some(connection) = self.connections.get_mut(&host) {
                            connection.appearance = Some((*appearance, provenance));
                        }
                    }
                    _ => {}
                },
                ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Error {
                    request_id,
                    error,
                    ..
                }) => {
                    if !self.report_command_failure(host, request_id, &error, cx) {
                        log::warn!(
                            target: "zz::diagnostics::mux",
                            "unsolicited_error host={host:?} error={error}"
                        );
                    }
                }
                ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Success {
                    request_id,
                    ..
                }) => {
                    if self.answer(host, request_id, Ok(())) {
                        cx.notify();
                    } else if let Some(connection) = self.connections.get(&host) {
                        connection.take_command(request_id);
                        connection.settle_command(request_id);
                    }
                }
                _ => {}
            }
            return;
        }
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let status_only = matches!(
            &message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::StatusChanged { .. },
                ..
            })
        );
        let quiet_success = self.error.is_none()
            && matches!(
                &message,
                ProtocolMessage::CommandResponse(CommandResponse::Success { .. })
            );
        let path_list = matches!(
            &message,
            ProtocolMessage::PathListBegin { .. }
                | ProtocolMessage::PathListChunk { .. }
                | ProtocolMessage::PathListGit { .. }
        );
        let terminal_frame = matches!(
            &message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { .. } | EventPayload::TerminalPatch { .. },
                ..
            })
        );
        log::trace!(
            target: "zz::diagnostics::mux",
            "handle_message begin message={message:#?}"
        );
        match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { pane, viewport },
                ..
            }) => self.apply_terminal_viewport(pane, viewport, cx),
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalPatch { pane, patch },
                ..
            }) => self.apply_terminal_patch(pane, patch, cx),
            ProtocolMessage::Event(Event {
                payload: EventPayload::CommandOutput { pane, viewport, .. },
                ..
            }) => self.apply_command_output(pane, viewport),
            message => {
                let attached_before = self.core.attached_session();
                self.core.handle_message(message);
                while let Some(outbound) = self.core.poll_outbound() {
                    match outbound {
                        Outbound::RequestFull(pane) => self.request_full_viewport(pane),
                        Outbound::TreeSync => {
                            if let Some(client) = &self.attached_connection().client {
                                let _ = client.send(&ProtocolMessage::TreeSync);
                            }
                        }
                    }
                }
                let mut attaching = false;
                while let Some(event) = self.core.poll_event() {
                    self.apply_core_event(host, event, attached_before, &mut attaching, cx);
                }
            }
        }
        log::trace!(
            target: "zz::diagnostics::mux",
            "handle_message end elapsed_us={} mux_generation={} viewports={} resync_pending={}",
            diagnostics::elapsed_us(started),
            self.core.snapshot().generation,
            self.viewports.len(),
            self.attached_connection().resync_pending,
        );
        if !status_only && !quiet_success && !path_list && !terminal_frame {
            cx.notify();
        }
    }

    fn apply_core_event(
        &mut self,
        host: HostId,
        event: CoreEvent,
        attached_before: Option<SessionId>,
        attaching: &mut bool,
        cx: &mut Context<Self>,
    ) {
        match event {
            CoreEvent::Attached { session } => {
                *attaching = true;
                self.finish_attach(host, session, cx);
            }
            CoreEvent::SnapshotChanged => {
                self.attached_snapshot_pending = false;
                let connection = self.attached_connection_mut();
                connection.resync_pending = false;
                connection.full_requests_pending.clear();
                connection.history.clear_deferred();
                if !*attaching {
                    self.backfill_retained_history();
                }
            }
            CoreEvent::AppearanceChanged => self.adopt_core_appearance(cx),
            CoreEvent::MuxOptionsChanged => self.backfill_retained_history(),
            CoreEvent::PrefixArmed { armed } => log::info!(
                target: "zz::diagnostics::input",
                "prefix_armed_received armed={armed}"
            ),
            CoreEvent::PrefixCancelled { request_id } => {
                self.prefix_cancelled_request = self.prefix_cancelled_request.max(Some(request_id));
            }
            CoreEvent::CommandPromptChanged => {
                self.command_prompt_revision = self.command_prompt_revision.wrapping_add(1).max(1);
            }
            CoreEvent::ChooseTreeChanged => {
                self.choose_tree_revision = self.choose_tree_revision.wrapping_add(1).max(1);
                if self.core.choose_tree().is_none() {
                    self.choose_tree_closed_revision = self.choose_tree_revision;
                }
            }
            CoreEvent::ChooseBufferChanged => {
                self.choose_buffer_revision = self.choose_buffer_revision.wrapping_add(1).max(1);
            }
            CoreEvent::DisplayPanesChanged => {
                self.display_panes_revision = self.display_panes_revision.wrapping_add(1).max(1);
            }
            CoreEvent::MenuChanged => {
                self.menu_revision = self.menu_revision.wrapping_add(1).max(1);
            }
            CoreEvent::ConfirmChanged => {
                self.confirm_revision = self.confirm_revision.wrapping_add(1).max(1);
            }
            CoreEvent::PaneRemoved { pane } => self.forget_pane(pane),
            CoreEvent::Bell { pane } => self.record_bell(host, pane),
            CoreEvent::FocusSidebar => {
                self.sidebar_focus_revision = self.sidebar_focus_revision.wrapping_add(1).max(1);
                log::info!(
                    target: "zz::diagnostics::input",
                    "focus_sidebar_received revision={}",
                    self.sidebar_focus_revision
                );
            }
            CoreEvent::Detached {
                session,
                by,
                action,
            } => {
                if attached_before == Some(session) {
                    self.reset_session_state(cx);
                    let notice = match action {
                        zz_protocol::ClientExitAction::Exec { command, .. } => {
                            format!("detach-client -E needs a terminal client: {command}")
                        }
                        _ => by.map_or_else(
                            || "session ended".to_owned(),
                            |device| format!("detached by {device}"),
                        ),
                    };
                    Self::emit_notification(ClientMessageKind::Warning, notice, cx);
                }
            }
            CoreEvent::ServerStopping => {
                self.error_after_next_attach = None;
                self.reset_session_state(cx);
                if host == HostId::LOCAL {
                    self.error = Some(Arc::from("zz daemon stopped"));
                } else {
                    self.error = None;
                }
            }
            CoreEvent::CommandResponse(response) => {
                self.handle_command_response(host, response, cx);
            }
            CoreEvent::ClientMessage {
                kind,
                text,
                duration_ms,
                message_id,
                ..
            } => cx.emit(ClientNotification {
                key: Some(if text.starts_with(CONFIG_RELOADED_MESSAGE) {
                    CONFIG_RELOADED_MESSAGE.to_owned()
                } else {
                    text.clone()
                }),
                kind,
                text,
                duration_ms,
                message_id,
            }),
            CoreEvent::ClientMessageCleared { message_id } => {
                cx.emit(ClientNotificationCleared { message_id });
            }
            CoreEvent::Clipboard {
                pane,
                producer,
                target,
                text,
                ..
            } => self.write_clipboard(pane, producer, target, text, cx),
            CoreEvent::OpenUri { pane, uri } => self.route_open_uri(pane, &uri, cx),
            CoreEvent::BrowserCommand { pane, command } => {
                if let BrowserCommand::Screenshot { request_id, path } = command {
                    self.screenshot_requests.push((pane, request_id, path));
                } else {
                    self.browser_commands.entry(pane).or_default().push(command);
                }
                self.pending_commands_revision =
                    self.pending_commands_revision.wrapping_add(1).max(1);
            }
            CoreEvent::TerminalUiCommand { pane, command } => {
                self.terminal_commands
                    .entry(pane)
                    .or_default()
                    .push(command);
                self.pending_commands_revision =
                    self.pending_commands_revision.wrapping_add(1).max(1);
            }
            CoreEvent::HistoryChunk {
                pane,
                start,
                total,
                offset,
                columns,
                rows,
                dictionary,
            } => self.apply_history_chunk_event(
                pane, start, total, offset, columns, rows, dictionary, cx,
            ),
            CoreEvent::KittyImageBegin {
                pane,
                image_id,
                generation,
                width,
                height,
                total_bytes,
            } => self.begin_kitty_image(pane, image_id, generation, width, height, total_bytes),
            CoreEvent::KittyImageChunk {
                pane,
                image_id,
                generation,
                bytes,
            } => self.push_kitty_image_chunk(pane, image_id, generation, &bytes),
            CoreEvent::KittyImagesRemoved { pane, image_ids } => {
                self.remove_kitty_images(pane, &image_ids);
            }
            CoreEvent::AgentUpdates {
                pane,
                first_seq,
                items,
            } => {
                self.apply_agent_updates(pane, first_seq, items);
            }
            CoreEvent::AgentStateChanged { pane, .. } => {
                cx.emit(AgentStateChanged);
                #[cfg(feature = "agent-pane")]
                if self.watched_agents.contains(&pane)
                    && let Some(state) = self.core.agent_state(pane).cloned()
                {
                    self.agent_events.states.push((pane, state));
                    if let std::collections::btree_map::Entry::Vacant(slot) =
                        self.agent_cursors.entry(pane)
                    {
                        slot.insert(0);
                        self.request_agent_replay_from_cursor(pane);
                    }
                }
                #[cfg(not(feature = "agent-pane"))]
                let _ = pane;
            }
            CoreEvent::AgentLagged { pane, next_seq } => {
                log::warn!(
                    target: "zz::diagnostics::mux",
                    "agent lane cleared for {pane} from {next_seq}; replaying from the shell's cursor"
                );
                #[cfg(feature = "agent-pane")]
                if self.watched_agents.contains(&pane) {
                    self.request_agent_replay_from_cursor(pane);
                }
            }
            CoreEvent::Message(message) => self.handle_unreduced_message(*message, cx),
            CoreEvent::KeyTableChanged => cx.emit(KeyTableChanged),
            CoreEvent::KeyTablesChanged
            | CoreEvent::HelloReceived
            | CoreEvent::ViewportChanged { .. }
            | CoreEvent::StatusChanged
            | CoreEvent::CommandOutputChanged
            | CoreEvent::ProvidersChanged
            | CoreEvent::AgentAccountsChanged
            | CoreEvent::AgentLoginChanged { .. }
            | CoreEvent::AgentReplayed { .. }
            | CoreEvent::SimulatorChanged { .. } => {}
            CoreEvent::SimulatorCommand { pane, command } => {
                self.simulator_commands.push((pane, command));
                self.pending_commands_revision =
                    self.pending_commands_revision.wrapping_add(1).max(1);
            }
            CoreEvent::AgentSessions { pane, result, .. } => {
                cx.emit(AgentCatalogReply(result.clone()));
                #[cfg(feature = "agent-pane")]
                if self.watched_agents.contains(&pane) {
                    self.agent_events.sessions.push((pane, result));
                }
                #[cfg(not(feature = "agent-pane"))]
                let _ = (pane, result);
            }
            CoreEvent::OpenPathPicker { pane, start_dir } => {
                cx.emit(PathPickerUpdate::Open { pane, start_dir });
            }
        }
    }

    fn backfill_retained_history(&mut self) {
        let panes = self.viewports.keys().copied().collect::<Vec<_>>();
        for pane in panes {
            self.request_history_backfill(pane);
        }
    }

    fn adopt_core_appearance(&mut self, cx: &mut Context<Self>) {
        let previous_hash = self.appearance.stable_hash();
        let host = (
            self.core.appearance().cloned().unwrap_or_default(),
            self.core.appearance_provenance().clone(),
        );
        self.attached_connection_mut().appearance = Some(host);
        let (mut appearance, provenance) = self.shown_appearance(cx);
        let configured_font_size = appearance.font_size_points;
        let requested_primary_font = appearance.font_families.first().cloned();
        localize_terminal_font_families(
            &mut appearance,
            &provenance,
            &self.available_font_names(cx),
        );
        self.terminal_font_size_offset_points =
            apply_terminal_font_size_offset(&mut appearance, self.terminal_font_size_offset_points);
        self.appearance = Arc::new(appearance);
        crate::theme::set_terminal_appearance(Arc::clone(&self.appearance), cx);
        self.command_output_diff.invalidate();
        log::info!(
            target: "zz::diagnostics::appearance",
            "reloaded appearance previous_hash={previous_hash} hash={} scheme={} requested_primary_font={requested_primary_font:?} primary_font={:?} configured_font_size_points={configured_font_size} effective_font_size_points={} font_size_offset_points={}",
            self.appearance.stable_hash(),
            self.appearance.color_scheme.as_str(),
            self.appearance.font_families.first(),
            self.appearance.font_size_points,
            self.terminal_font_size_offset_points,
        );
    }

    fn forget_pane(&mut self, pane: PaneId) {
        self.viewports.remove(&pane);
        self.clear_kitty_images(pane);
        let connection = self.attached_connection_mut();
        connection.full_requests_pending.remove(&pane);
        connection.history.forget(pane);
        self.browser_commands.remove(&pane);
        self.terminal_commands.remove(&pane);
        #[cfg(feature = "agent-pane")]
        self.unwatch_agent(pane);
        self.pane_images.remove(&pane);
        self.pasted_image_assemblies
            .retain(|(target, _), _| *target != pane);
        self.pending_pasted_image_previews
            .retain(|(target, _)| *target != pane);
        let mut removed_screenshots = Vec::new();
        self.screenshot_requests.retain(|(target, request_id, _)| {
            let keep = *target != pane;
            if !keep {
                removed_screenshots.push(*request_id);
            }
            keep
        });
        for request_id in removed_screenshots {
            self.respond_to_request(GuiResponse::Error {
                request_id,
                message: format!("{pane} was closed"),
            });
        }
    }

    fn route_open_uri(&mut self, pane: PaneId, uri: &str, cx: &mut Context<Self>) {
        let route = open_uri_route(
            self.core.snapshot(),
            self.core.attached_session(),
            pane,
            uri,
        );
        match route {
            OpenUriRoute::Embedded { pane: browser, url } => {
                log::debug!(
                    "routing terminal link from {pane} to embedded browser {browser}: {}",
                    diagnostic_url(&url)
                );
                self.browser_commands
                    .entry(browser)
                    .or_default()
                    .push(BrowserCommand::Navigate(url));
                self.pending_commands_revision =
                    self.pending_commands_revision.wrapping_add(1).max(1);
            }
            OpenUriRoute::PastedImage { pane, number } => self.open_pasted_image(pane, number, cx),
            OpenUriRoute::External => cx.open_url(uri),
        }
    }

    fn handle_unreduced_message(&mut self, message: ProtocolMessage, cx: &mut Context<Self>) {
        match message {
            ProtocolMessage::PastedImageBegin {
                pane,
                number,
                format,
                total_bytes,
            } => self.begin_pasted_image(pane, number, format, total_bytes),
            ProtocolMessage::PastedImageChunk {
                pane,
                number,
                bytes,
            } => self.push_pasted_image_chunk(pane, number, &bytes, cx),
            ProtocolMessage::PastedImageUnavailable { pane, number } => {
                self.pasted_image_unavailable(pane, number);
            }
            ProtocolMessage::ArtifactBegin {
                request_id,
                mime,
                total_bytes,
                reply,
            } => {
                if let Some(fetch) = self.artifact_fetches.get_mut(&request_id) {
                    let total = usize::try_from(total_bytes).unwrap_or(usize::MAX);
                    fetch.bytes.reserve(total);
                    fetch.begun = Some(ArtifactBegun { mime, total, reply });
                }
                self.artifact_progress(request_id);
            }
            ProtocolMessage::ArtifactChunk { request_id, bytes } => {
                if let Some(fetch) = self.artifact_fetches.get_mut(&request_id) {
                    fetch.bytes.extend_from_slice(&bytes);
                }
                self.artifact_progress(request_id);
            }
            ProtocolMessage::ArtifactUnavailable { request_id, reason } => {
                if let Some(fetch) = self.artifact_fetches.remove(&request_id) {
                    fetch.request.respond(Err(reason));
                }
            }
            _ => {}
        }
    }

    fn apply_terminal_viewport(
        &mut self,
        pane: PaneId,
        viewport: TerminalViewport,
        cx: &mut Context<Self>,
    ) {
        self.kitty_image_cache(pane);
        self.viewports.insert(
            pane,
            Arc::new(RwLock::new(new_retained_viewport(
                viewport,
                &mut self.next_row_revision,
            ))),
        );
        let connection = self.attached_connection_mut();
        connection.full_requests_pending.remove(&pane);
        connection.history.forget(pane);
        self.request_history_backfill(pane);
        self.signal_terminal(pane, cx);
    }

    fn apply_terminal_patch(
        &mut self,
        pane: PaneId,
        patch: TerminalViewportPatch,
        cx: &mut Context<Self>,
    ) {
        self.kitty_image_cache(pane);
        let retained = self.viewports.get(&pane).cloned();
        let request_missing =
            retained.is_none() && snapshot_contains_pane(self.core.snapshot(), pane);
        let request_failed_patch = retained.is_some();
        let apply_result = if let Some(retained) = retained {
            let mut retained = retained.write();
            if retained.row_revisions.len() == usize::from(patch.rows) {
                apply_retained_patch(&mut retained, patch, &mut self.next_row_revision).map_err(
                    |error| {
                        log::warn!("rejected terminal patch for {pane}: {error}");
                    },
                )
            } else {
                log::warn!("terminal row revisions are out of sync for {pane}");
                Err(())
            }
        } else {
            log::warn!("received terminal patch for missing pane {pane}");
            Err(())
        };
        if apply_result.is_err() && (request_failed_patch || request_missing) {
            self.request_full_viewport(pane);
        } else if apply_result.is_ok() {
            self.request_history_backfill(pane);
            self.signal_terminal(pane, cx);
        }
    }

    fn request_full_viewport(&mut self, pane: PaneId) {
        let connection = self.attached_connection_mut();
        if connection.full_requests_pending.insert(pane)
            && let Some(client) = &connection.client
            && let Err(error) = client.request_full(pane)
        {
            connection.full_requests_pending.remove(&pane);
            log::warn!("failed to request a full terminal viewport for {pane}: {error}");
        }
    }

    fn apply_command_output(&mut self, pane: PaneId, viewport: Option<TerminalViewport>) {
        let Some(viewport) = viewport else {
            let output_pane = self
                .command_output
                .as_ref()
                .map_or(pane, |output| output.pane);
            self.command_output = None;
            self.command_output_diff.invalidate();
            self.terminal_commands.remove(&output_pane);
            return;
        };
        let Some(output) = self
            .command_output
            .as_ref()
            .filter(|output| output.pane == pane)
        else {
            self.command_output_diff.invalidate();
            self.command_output = Some(CommandOutputModel {
                pane,
                retained: Arc::new(RwLock::new(new_retained_viewport(
                    viewport,
                    &mut self.next_row_revision,
                ))),
            });
            return;
        };
        let mut retained = output.retained.write();
        let patch = TerminalViewport::diff_with_scratch(
            &retained.viewport,
            &viewport,
            &mut self.command_output_diff,
        );
        if let Some(patch) = patch {
            if let Err(error) =
                apply_retained_patch(&mut retained, patch, &mut self.next_row_revision)
            {
                log::warn!("could not retain command-output rows for {pane}: {error}");
                replace_retained_viewport(&mut retained, viewport, &mut self.next_row_revision);
                self.command_output_diff.invalidate();
            } else {
                self.command_output_diff
                    .remember_applied(&retained.viewport);
            }
        } else {
            replace_retained_viewport(&mut retained, viewport, &mut self.next_row_revision);
            self.command_output_diff.invalidate();
        }
    }

    fn apply_history_chunk_event(
        &mut self,
        pane: PaneId,
        start: u32,
        total: u32,
        offset: u32,
        columns: u16,
        rows: Vec<Vec<PackedCell>>,
        dictionary: TerminalDictionary,
        cx: &mut Context<Self>,
    ) {
        let retained = self.viewports.get(&pane).cloned();
        let mutations = retained
            .as_ref()
            .map(|retained| retained.read().history_mutations);
        let (apply, follow_up) = self
            .attached_connection_mut()
            .history
            .chunk_arrived(pane, mutations);
        if apply && let Some(retained) = retained {
            apply_history_chunk(
                &mut retained.write(),
                start,
                total,
                offset,
                columns,
                rows,
                dictionary,
                &mut self.next_row_revision,
            );
        }
        match follow_up {
            HistoryFollowUp::Prefetch(target) => self.request_history_prefetch(pane, target),
            HistoryFollowUp::Defer(mutations) => self.defer_history_backfill(pane, mutations, cx),
            HistoryFollowUp::Backfill => self.request_history_backfill(pane),
        }
    }

    fn record_bell(&mut self, host: HostId, pane: PaneId) {
        log::debug!(target: "zz::diagnostics::mux", "bell rang host={host:?} pane={pane}");
        self.bell_revision = self.bell_revision.wrapping_add(1).max(1);
    }
}

impl EventEmitter<InitialConnectionFinished> for MuxClient {}
impl EventEmitter<AgentStateChanged> for MuxClient {}
impl EventEmitter<CommandPrinted> for MuxClient {}
impl EventEmitter<AgentCatalogReply> for MuxClient {}
impl EventEmitter<KeyTableChanged> for MuxClient {}
impl EventEmitter<ClientNotification> for MuxClient {}
impl EventEmitter<ClientNotificationCleared> for MuxClient {}

impl EventEmitter<AttachmentPreviewRequest> for MuxClient {}

impl EventEmitter<SshPromptRequest> for MuxClient {}

impl EventEmitter<HostUpdateQuestion> for MuxClient {}

impl EventEmitter<PathPickerUpdate> for MuxClient {}

impl EventEmitter<HostPathListUpdate> for MuxClient {}

fn adjusted_terminal_font_size(current: f32, adjustment: TerminalFontSizeAdjustment) -> f32 {
    (current + adjustment.delta_points())
        .clamp(MIN_TERMINAL_FONT_SIZE_POINTS, MAX_TERMINAL_FONT_SIZE_POINTS)
}

fn apply_terminal_font_size_offset(appearance: &mut TerminalAppearance, offset_points: f32) -> f32 {
    let configured = appearance.font_size_points;
    let effective = (configured + offset_points)
        .clamp(MIN_TERMINAL_FONT_SIZE_POINTS, MAX_TERMINAL_FONT_SIZE_POINTS);
    appearance.font_size_points = effective;
    effective - configured
}

#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod appearance_tests;
#[cfg(test)]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "the fake host is Cell-based and fills a slot typed for the real Arc'd client"
)]
mod attach_tests;
#[cfg(test)]
mod commands_tests;
#[cfg(test)]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "the fake host is Cell-based and fills a slot typed for the real Arc'd client"
)]
mod connection_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod images_tests;
#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod messages_tests;
#[cfg(test)]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "the fake host is Cell-based and fills a slot typed for the real Arc'd client"
)]
mod test_support;

#[cfg(test)]
mod tests {

    pub(super) struct KittyPaneLifecycleRoot {
        pub(super) terminal: Option<ala::Entity<crate::terminal::view::TerminalView>>,
        pub(super) other: Option<ala::Entity<crate::terminal::view::TerminalView>>,
        pub(super) shown: bool,
    }

    impl ala::Render for KittyPaneLifecycleRoot {
        fn render(
            &mut self,
            _window: &mut ala::Window,
            _cx: &mut ala::Context<Self>,
        ) -> impl ala::IntoElement {
            use ala::prelude::*;
            ala::div()
                .size_full()
                .flex()
                .child(
                    ala::div()
                        .flex_1()
                        .h_full()
                        .children(self.shown.then(|| self.terminal.clone()).flatten()),
                )
                .child(ala::div().flex_1().h_full().children(self.other.clone()))
        }
    }

    pub(super) struct KittyPaneHandoffRoot {
        pub(super) terminal: ala::Entity<crate::terminal::view::TerminalView>,
    }

    impl ala::Render for KittyPaneHandoffRoot {
        fn render(
            &mut self,
            _window: &mut ala::Window,
            _cx: &mut ala::Context<Self>,
        ) -> impl ala::IntoElement {
            use ala::prelude::*;
            ala::div().size_full().child(
                self.terminal
                    .clone()
                    .cached(ala::StyleRefinement::default().size_full()),
            )
        }
    }
}
