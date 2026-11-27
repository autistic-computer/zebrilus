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

mod connection;
mod daemon_restart;
mod host_update;
mod overlays;

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

    pub(crate) fn send_input(&self, input: InputMessage) -> bool {
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        log::trace!(target: "zz::diagnostics::mux", "send_input begin input={input:#?}");
        #[cfg(test)]
        if let Some(sink) = &self.input_sink {
            sink.borrow_mut().push(input);
            return true;
        }
        let input = match input {
            InputMessage::ResizeTerminal {
                pane,
                columns,
                rows,
                cell_width_px,
                cell_height_px,
            } => InputMessage::ResizeTerminalV2 {
                pane,
                columns,
                rows,
                cell_width_px,
                cell_height_px,
                layout_generation: self.core.layout_generation(),
            },
            InputMessage::ClientTerminalSize { columns, rows } => {
                InputMessage::ClientTerminalSizeV2 {
                    columns,
                    rows,
                    layout_generation: self.core.layout_generation(),
                }
            }
            input => input,
        };
        let sent = if let Some(client) = &self.attached_connection().client {
            if let Err(error) = client.send_input(input) {
                log::warn!("failed to send mux input: {error}");
                false
            } else {
                true
            }
        } else {
            log::trace!(target: "zz::diagnostics::mux", "send_input skipped: no client");
            false
        };
        log::trace!(
            target: "zz::diagnostics::mux",
            "send_input end elapsed_us={}",
            diagnostics::elapsed_us(started)
        );
        sent
    }

    pub(crate) fn set_client_window_focused(&mut self, focused: bool) {
        self.desired_client_window_focus = Some(focused);
        self.flush_client_window_focus();
    }

    fn begin_client_focus_attach(&mut self) {
        if !self.client_focus_attach_pending {
            self.client_focus_attach_recoverable = self.client_focus_attach_ready;
        }
        self.client_focus_attach_ready = false;
        self.client_focus_attach_pending = true;
    }

    fn reset_client_focus_attach(&mut self) {
        self.client_focus_attach_ready = false;
        self.client_focus_attach_pending = false;
        self.client_focus_attach_recoverable = false;
        self.sent_client_window_focus = None;
    }

    fn fail_client_focus_attach(&mut self) {
        if !self.client_focus_attach_pending {
            return;
        }
        self.client_focus_attach_pending = false;
        let recoverable = std::mem::take(&mut self.client_focus_attach_recoverable);
        if recoverable {
            self.client_focus_attach_ready = true;
            self.flush_client_window_focus();
        } else {
            self.reset_client_focus_attach();
        }
    }

    fn flush_client_window_focus(&mut self) {
        let Some(focused) = self.desired_client_window_focus else {
            return;
        };
        if !self.client_focus_attach_ready || self.sent_client_window_focus == Some(focused) {
            return;
        }
        if self.send_input(InputMessage::ClientFocus { focused }) {
            self.sent_client_window_focus = Some(focused);
        }
    }

    pub(crate) fn request_host_path_list(
        &mut self,
        host: HostId,
        pane: Option<PaneId>,
        dir: String,
    ) -> Option<u64> {
        #[cfg(test)]
        if let Some(sink) = &self.path_list_sink {
            let sent = sink
                .borrow()
                .iter()
                .filter(|message| matches!(message, ProtocolMessage::PathListRequest { .. }))
                .count();
            let request_id = u64::try_from(sent).unwrap_or(u64::MAX) + 1;
            sink.borrow_mut().push(ProtocolMessage::PathListRequest {
                request_id,
                pane,
                dir: Some(dir),
            });
            self.host_path_lists.insert((host, request_id));
            return Some(request_id);
        }
        let client = self.connections.get(&host)?.client.as_ref()?;
        match client.request_path_list(pane, Some(dir)) {
            Ok(request_id) => {
                self.host_path_lists.insert((host, request_id));
                Some(request_id)
            }
            Err(error) => {
                log::warn!("failed to request a path listing on {host:?}: {error}");
                None
            }
        }
    }

    pub(crate) fn cancel_host_path_list(&mut self, host: HostId, request_id: u64) {
        #[cfg(test)]
        if let Some(sink) = &self.path_list_sink {
            if self.host_path_lists.remove(&(host, request_id)) {
                sink.borrow_mut()
                    .push(ProtocolMessage::PathListCancel { request_id });
            }
            return;
        }
        if self.host_path_lists.remove(&(host, request_id))
            && let Some(client) = self
                .connections
                .get(&host)
                .and_then(|connection| connection.client.as_ref())
            && let Err(error) = client.cancel_path_list(request_id)
        {
            log::warn!("failed to cancel path listing {request_id} on {host:?}: {error}");
        }
    }

    fn route_host_path_list(
        &mut self,
        host: HostId,
        message: ProtocolMessage,
        cx: &mut Context<Self>,
    ) -> Option<ProtocolMessage> {
        let asked = |request_id: &u64| self.host_path_lists.contains(&(host, *request_id));
        let (request_id, finished, update) = match message {
            ProtocolMessage::PathListBegin { request_id, result } if asked(&request_id) => (
                request_id,
                result.is_err(),
                PathPickerUpdate::Begin { request_id, result },
            ),
            ProtocolMessage::PathListChunk {
                request_id,
                entries,
                done,
                truncated,
            } if asked(&request_id) => (
                request_id,
                done,
                PathPickerUpdate::Chunk {
                    request_id,
                    entries: entries.into(),
                    done,
                    truncated,
                },
            ),
            ProtocolMessage::PathListGit { request_id, marks } if asked(&request_id) => (
                request_id,
                false,
                PathPickerUpdate::Git { request_id, marks },
            ),
            message => return Some(message),
        };
        if finished {
            self.host_path_lists.remove(&(host, request_id));
        }
        cx.emit(HostPathListUpdate { host, update });
        None
    }

    pub(crate) fn paste_to_pane(&self, pane: PaneId, text: String) -> bool {
        self.send_input(InputMessage::TerminalView {
            pane,
            action: TerminalViewAction::Paste(text),
        })
    }

    pub(crate) fn send_prefix_cancel(&mut self) -> Option<u64> {
        let request_id = self.next_prefix_cancel_request.saturating_add(1).max(1);
        if self.send_input(InputMessage::CancelPrefix { request_id }) {
            self.next_prefix_cancel_request = request_id;
            Some(request_id)
        } else {
            None
        }
    }

    pub(crate) fn send_paste_upload(
        &self,
        upload_id: u64,
        pane: PaneId,
        extension: String,
        bytes: &[u8],
    ) -> bool {
        let Some(client) = &self.attached_connection().client else {
            log::trace!(target: "zz::diagnostics::mux", "paste upload skipped: no client");
            return false;
        };
        if let Err(error) = client.send_paste_upload(upload_id, pane, extension, bytes) {
            log::warn!("failed to upload a pasted image: {error}");
            return false;
        }
        true
    }

    pub(crate) fn record_pasted_image(
        &self,
        upload_id: u64,
        pane: PaneId,
        extension: String,
        bytes: &[u8],
    ) -> bool {
        let Some(client) = &self.attached_connection().client else {
            log::trace!(target: "zz::diagnostics::mux", "pasted image record skipped: no client");
            return false;
        };
        if let Err(error) = client.record_pasted_image(upload_id, pane, extension, bytes) {
            log::warn!("failed to record a pasted image: {error}");
            return false;
        }
        true
    }

    pub(crate) fn attached_is_local(&self) -> bool {
        self.attached_host == HostId::LOCAL
    }

    pub fn execute(&self, command: CommandInvocation) {
        self.execute_on_host(self.attached_host, command);
    }

    pub fn execute_tracked(&self, command: CommandInvocation) -> Option<u64> {
        let request_id = self.execute_on_host(self.attached_host, command)?;
        self.attached_connection().watch_command(request_id);
        Some(request_id)
    }

    pub fn execute_answered(&self, command: CommandInvocation) -> Option<u64> {
        let request_id = self.execute_on_host(self.attached_host, command)?;
        self.attached_connection().answer_command(request_id);
        Some(request_id)
    }

    pub fn execute_printed(&self, command: CommandInvocation) -> Option<u64> {
        let request_id = self.execute_on_host(self.attached_host, command)?;
        self.printed_commands.borrow_mut().insert(request_id);
        Some(request_id)
    }

    pub(crate) fn has_command_answers(&self) -> bool {
        !self.command_answers.is_empty()
    }

    pub(crate) fn take_command_answers(&mut self) -> Vec<(u64, Result<(), String>)> {
        std::mem::take(&mut self.command_answers)
    }

    fn answer(&mut self, host: HostId, request_id: u64, result: Result<(), String>) -> bool {
        if request_id == 0
            || !self
                .connections
                .get(&host)
                .is_some_and(|connection| connection.take_answered(request_id))
        {
            return false;
        }
        if let Some(connection) = self.connections.get(&host) {
            connection.take_command(request_id);
            connection.settle_command(request_id);
        }
        self.command_answers.push((request_id, result));
        true
    }

    #[must_use]
    pub fn command_settled(&self, request_id: u64) -> bool {
        self.attached_connection()
            .watched_commands
            .read()
            .iter()
            .any(|&(id, settled)| id == request_id && settled)
    }

    #[cfg(test)]
    pub(crate) fn watch_command_for_test(&self, request_id: u64) {
        self.attached_connection().watch_command(request_id);
    }

    pub fn execute_on_host(&self, host: HostId, command: CommandInvocation) -> Option<u64> {
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        log::trace!(
            target: "zz::diagnostics::mux",
            "execute begin host={host:?} command={command:#?}",
        );
        let Some(connection) = self.connections.get(&host) else {
            log::warn!("cannot send mux command to unknown fleet host {host:?}");
            return None;
        };
        let name = command.name.clone();
        let sent = if let Some(client) = &connection.client {
            client.execute(command)
        } else {
            #[cfg(test)]
            if let Some(client) = &connection.fake_client {
                let sent = client.execute(command);
                if let Ok(request_id) = sent {
                    connection.track_command(request_id, name);
                }
                return sent.ok();
            }
            log::trace!(
                target: "zz::diagnostics::mux",
                "execute skipped: no client for host={host:?}",
            );
            return None;
        };
        log::trace!(
            target: "zz::diagnostics::mux",
            "execute end elapsed_us={}",
            diagnostics::elapsed_us(started)
        );
        match sent {
            Ok(request_id) => {
                connection.track_command(request_id, name);
                Some(request_id)
            }
            Err(error) => {
                log::warn!("failed to send mux command: {error}");
                None
            }
        }
    }

    pub(crate) fn host_providers(&self, host: HostId) -> Option<&zz_protocol::ProvidersReport> {
        self.connections.get(&host)?.providers.as_ref()
    }

    pub(crate) fn providers_unrequested(&self) -> bool {
        self.connections.values().any(|connection| {
            matches!(connection.state, HostState::Connected) && !connection.providers_requested
        })
    }

    pub(crate) fn request_providers(&mut self, force: bool) {
        let hosts = self
            .connections
            .iter_mut()
            .filter(|(_, connection)| {
                matches!(connection.state, HostState::Connected)
                    && (force || !connection.providers_requested)
            })
            .map(|(host, connection)| {
                connection.providers_requested = true;
                *host
            })
            .collect::<Vec<_>>();
        for host in hosts {
            self.execute_on_host(
                host,
                CommandInvocation::new("providers", Vec::<String>::new()),
            );
            self.request_agent_accounts(host);
        }
    }

    pub(crate) fn update_provider(&self, host: HostId, provider: zz_protocol::AgentProvider) {
        self.execute_on_host(
            host,
            CommandInvocation::new("providers", ["-u", provider.as_str()]),
        );
    }

    pub(crate) fn host_agent_accounts(&self, host: HostId) -> Option<&[zz_protocol::AgentAccount]> {
        self.connections.get(&host)?.agent_accounts.as_deref()
    }

    pub(crate) fn host_agent_quotas(&self, host: HostId) -> &[zz_protocol::AgentQuota] {
        self.connections
            .get(&host)
            .map_or(&[], |connection| &connection.agent_quotas)
    }

    pub(crate) fn request_agent_accounts(&self, host: HostId) {
        if !self.host_platform(host).runs_agents() {
            return;
        }
        self.execute_on_host(
            host,
            CommandInvocation::new("agent-accounts", Vec::<String>::new()),
        );
    }

    pub(crate) fn host_signs_in(&self, host: HostId) -> bool {
        self.connections
            .get(&host)
            .and_then(|connection| connection.client.as_ref())
            .is_some_and(|client| {
                zz_client::agent_login::daemon_signs_in(&client.server_hello().capabilities)
            })
    }

    pub(crate) fn start_agent_login(
        &self,
        host: HostId,
        provider: zz_protocol::AgentProvider,
        account: zz_client::agent_login::LoginAccount<'_>,
        pane: PaneId,
    ) {
        let capabilities = self
            .connections
            .get(&host)
            .and_then(|connection| connection.client.as_ref())
            .map_or(&[][..], |client| {
                client.server_hello().capabilities.as_slice()
            });
        let command = zz_client::agent_login::entry(
            provider,
            account,
            pane,
            zz_client::agent_login::browser(host == HostId::LOCAL),
            capabilities,
        );
        self.execute_on_host(host, command);
    }

    pub(crate) fn start_provider_login(
        &self,
        host: HostId,
        provider: zz_protocol::AgentProvider,
        account: zz_client::agent_login::LoginAccount<'_>,
    ) {
        if !self.host_signs_in(host) {
            return;
        }
        let command = zz_client::agent_login::start(
            provider,
            account,
            None,
            zz_client::agent_login::browser(host == HostId::LOCAL),
        );
        self.execute_on_host(host, command);
    }

    pub(crate) fn host_agent_login(
        &self,
        host: HostId,
        pane: PaneId,
    ) -> Option<&zz_protocol::AgentLogin> {
        zz_client::agent_login::for_pane(&self.connections.get(&host)?.agent_logins, pane)
    }

    pub(crate) fn host_provider_login(
        &self,
        host: HostId,
        provider: zz_protocol::AgentProvider,
    ) -> Option<&zz_protocol::AgentLogin> {
        zz_client::agent_login::for_provider(&self.connections.get(&host)?.agent_logins, provider)
    }

    pub(crate) fn dismiss_agent_login(&mut self, host: HostId, login: u64) {
        if let Some(connection) = self.connections.get_mut(&host) {
            connection
                .agent_logins
                .retain(|known| known.id != login || !known.finished());
        }
    }

    pub(crate) fn send_agent_login(&self, host: HostId, message: &ProtocolMessage) {
        let Some(client) = self
            .connections
            .get(&host)
            .and_then(|connection| connection.client.as_ref())
        else {
            return;
        };
        if let Err(error) = client.send(message) {
            log::warn!(target: "zz::agent", "could not send a sign-in request: {error}");
        }
    }

    fn observe_agent_login(
        &mut self,
        host: HostId,
        login: zz_protocol::AgentLogin,
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self.connections.get_mut(&host) else {
            return;
        };
        let open = match &login.step {
            zz_protocol::AgentLoginStep::OpenUrl { url }
            | zz_protocol::AgentLoginStep::PasteCode { url }
                if host == HostId::LOCAL
                    && login.browser == zz_protocol::AgentLoginBrowser::Local
                    && login.provider != zz_protocol::AgentProvider::Grok
                    && connection.opened_logins.insert(login.id) =>
            {
                Some(url.clone())
            }
            _ => None,
        };
        if zz_client::agent_login::reduce(&mut connection.agent_logins, login) {
            cx.notify();
        }
        if let Some(url) = open {
            cx.open_url(&url);
        }
    }

    pub fn new_session(&self, host: HostId) {
        let Some(connection) = self.connections.get(&host) else {
            log::warn!("cannot create a session on unknown fleet host {host:?}");
            return;
        };
        let capabilities = connection.client.as_ref().map_or(&[][..], |client| {
            client.server_hello().capabilities.as_slice()
        });
        #[cfg(test)]
        let capabilities = connection
            .fake_client
            .as_ref()
            .map_or(capabilities, |client| client.hello.capabilities.as_slice());
        let snapshot = if host == self.attached_host && !self.attached_snapshot_pending {
            Some(self.core.snapshot().as_ref())
        } else {
            connection.snapshot.as_deref()
        };
        let working_directory = (host == HostId::LOCAL
            && snapshot.is_some_and(|snapshot| snapshot.sessions.is_empty()))
        .then(std::env::current_dir)
        .transpose()
        .ok()
        .flatten()
        .filter(|directory| directory.is_dir());
        for command in new_session_commands(host, capabilities, working_directory.as_deref()) {
            self.execute_on_host(host, command);
        }
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

    pub(crate) fn attach(&mut self, session: SessionId) {
        let client = self.attached_connection().client.as_ref().map(Arc::clone);
        #[cfg(test)]
        let fake_client = self
            .attached_connection()
            .fake_client
            .as_ref()
            .map(Arc::clone);
        #[cfg(not(test))]
        let result = client.map(|client| client.attach(session.to_string()));
        #[cfg(test)]
        let result = client
            .map(|client| client.attach(session.to_string()))
            .or_else(|| fake_client.map(|client| client.attach(Some(session))));
        match result {
            Some(Ok(())) => self.begin_client_focus_attach(),
            Some(Err(error)) => log::warn!("failed to attach mux session: {error}"),
            None => {}
        }
    }

    pub fn attach_to_host(
        &mut self,
        host: HostId,
        session: SessionId,
        cx: &mut Context<Self>,
    ) -> bool {
        self.attach_to_host_target(host, Some(session), None, cx)
    }

    pub fn attach_to_host_at(
        &mut self,
        host: HostId,
        target: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.attach_to_host_target(host, None, Some(target), cx)
    }

    pub fn attach_to_host_default(&mut self, host: HostId, cx: &mut Context<Self>) -> bool {
        self.attach_to_host_target(host, None, None, cx)
    }

    pub(crate) fn land(
        &mut self,
        host: HostId,
        landing: zz_client::new_chat::Landing,
        cx: &mut Context<Self>,
    ) -> bool {
        for command in landing.commands {
            self.execute_on_host(host, command);
        }
        self.attach_to_host_at(host, landing.attach, cx)
    }

    pub(crate) fn release_host(&mut self, host: HostId, cx: &mut Context<Self>) {
        if self.attached_host == host {
            if self.registry.get(HostId::LOCAL).is_some() {
                self.attach_to_host_default(HostId::LOCAL, cx);
            } else {
                let outgoing_snapshot = Arc::clone(self.core.snapshot());
                self.cancel_reconnect(host);
                self.attached_connection_mut().snapshot = Some(outgoing_snapshot);
                if let Some(client) = &self.attached_connection().client {
                    let _ = client.detach();
                }
                self.reset_session_state(cx);
                self.clear_cross_host_state();
                self.attached_host = HostId::LOCAL;
                self.error = None;
                self.error_after_next_attach = None;
                cx.notify();
            }
        }
    }

    fn attach_to_host_target(
        &mut self,
        host: HostId,
        session: Option<SessionId>,
        target: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if host == self.attached_host {
            if let Some(target) = target {
                self.execute_on_host(
                    host,
                    CommandInvocation::new("attach-session", ["-t", &target]),
                );
            } else if let Some(session) = session {
                self.attach(session);
            }
            return true;
        }

        let requested_name = self.registry.get(host).map(|entry| entry.name.clone());
        self.reconcile_hosts(cx);
        let Some((host, name)) = requested_name
            .as_deref()
            .and_then(|name| self.registry.get_by_name(name))
            .map(|(host, entry)| (host, entry.name.clone()))
        else {
            log::warn!("cannot attach to unknown fleet host {host:?}");
            return false;
        };
        let Some(connection) = self.connections.get(&host) else {
            log::warn!("cannot attach to fleet host {name}: no connection slot");
            return false;
        };
        if !connection.is_connected() {
            log::warn!(
                "cannot attach to fleet host {name}: host is {}",
                connection.state.label(),
            );
            return false;
        }
        let client = connection.client.as_ref().map(Arc::clone);
        #[cfg(test)]
        let fake_client = connection.fake_client.as_ref().map(Arc::clone);
        let hello = connection
            .current_hello()
            .expect("a connected host has an interactive client");

        let outgoing_snapshot = Arc::clone(self.core.snapshot());
        self.cancel_reconnect(self.attached_host);
        self.attached_connection_mut().snapshot = Some(outgoing_snapshot);
        if let Some(current) = &self.attached_connection().client {
            let _ = current.detach();
        }
        self.reset_session_state(cx);

        self.clear_cross_host_state();

        self.error = None;
        self.error_after_next_attach = None;
        self.attached_host = host;
        self.report_color_scheme();
        self.ingest_server_hello(hello, cx);
        if let Some(target) = target {
            self.execute_on_host(
                host,
                CommandInvocation::new("attach-session", ["-t", &target]),
            );
            self.reconcile_hosts(cx);
            cx.notify();
            return true;
        }
        let attach_result = if let Some(client) = client {
            client.attach(session.map_or_else(String::new, |session| session.to_string()))
        } else {
            #[cfg(test)]
            {
                fake_client
                    .expect("a connected test host has a fake client")
                    .attach(session)
            }
            #[cfg(not(test))]
            unreachable!("a connected host has an interactive client");
        };
        match attach_result {
            Ok(()) => self.begin_client_focus_attach(),
            Err(error) => {
                self.reset_client_focus_attach();
                log::warn!("failed to attach mux session on fleet host {name}: {error}");
            }
        }
        self.reconcile_hosts(cx);
        cx.notify();
        true
    }

    fn give_up_removed_attached_host(&mut self, name: &str, cx: &mut Context<Self>) {
        debug_assert_eq!(self.attached_host, HostId::LOCAL);
        self.reset_session_state(cx);
        self.clear_cross_host_state();
        let banner = Arc::<str>::from(format!("fleet host {name} disconnected"));
        self.error = Some(Arc::clone(&banner));
        self.error_after_next_attach = None;

        let connection = self.attached_connection();
        if !connection.is_connected() {
            return;
        }
        let client = connection.client.as_ref().map(Arc::clone);
        #[cfg(test)]
        let fake_client = connection.fake_client.as_ref().map(Arc::clone);
        let hello = connection
            .current_hello()
            .expect("a connected host has an interactive client");
        self.ingest_server_hello(hello, cx);
        let result = if let Some(client) = client {
            client.attach("")
        } else {
            #[cfg(test)]
            {
                fake_client
                    .expect("a connected test host has a fake client")
                    .attach(None)
            }
            #[cfg(not(test))]
            unreachable!("a connected host has an interactive client");
        };
        match result {
            Ok(()) => {
                self.begin_client_focus_attach();
                self.error_after_next_attach = Some(banner);
            }
            Err(error) => {
                self.reset_client_focus_attach();
                log::warn!("failed to fall back after removing fleet host {name}: {error}");
            }
        }
    }

    fn clear_cross_host_state(&mut self) {
        self.reset_client_focus_attach();
        self.core.clear_attachment();
        self.attached_snapshot_pending = true;
        self.viewports.clear();
        self.clear_all_kitty_images();
        self.browser_commands.clear();
        self.clear_agent_streams();
        self.screenshot_requests.clear();
        self.simulator_commands.clear();
        self.terminal_commands.clear();
        self.clear_all_pasted_images();
        self.pending_commands_revision = self.pending_commands_revision.wrapping_add(1).max(1);
        self.command_output = None;
        self.command_output_diff.invalidate();
        self.menu_revision = self.menu_revision.wrapping_add(1).max(1);
        self.confirm_revision = self.confirm_revision.wrapping_add(1).max(1);
    }

    fn reset_session_state(&mut self, cx: &mut Context<Self>) {
        let key_table_changed = self.core.key_table().is_some();
        cx.emit(PathPickerUpdate::Close);
        self.reset_client_focus_attach();
        let connection = self.attached_connection_mut();
        connection.resync_pending = false;
        connection.full_requests_pending.clear();
        connection.history.clear();
        if self.next_prefix_cancel_request != 0 {
            self.prefix_cancelled_request = self
                .prefix_cancelled_request
                .max(Some(self.next_prefix_cancel_request));
        }
        self.core.reset_session();
        self.command_prompt_revision = self.command_prompt_revision.wrapping_add(1).max(1);
        self.command_output = None;
        self.choose_tree_revision = self.choose_tree_revision.wrapping_add(1).max(1);
        self.choose_tree_closed_revision = self.choose_tree_revision;
        self.choose_buffer_revision = self.choose_buffer_revision.wrapping_add(1).max(1);
        self.display_panes_revision = self.display_panes_revision.wrapping_add(1).max(1);
        self.menu_revision = self.menu_revision.wrapping_add(1).max(1);
        self.confirm_revision = self.confirm_revision.wrapping_add(1).max(1);
        self.clear_all_kitty_images();
        self.clear_all_pasted_images();
        for fetch in std::mem::take(&mut self.artifact_fetches).into_values() {
            fetch.request.respond(Err(
                "zz lost this page's daemon connection; reload to try again".to_owned(),
            ));
        }
        if key_table_changed {
            cx.emit(KeyTableChanged);
        }
    }

    fn kitty_image_cache(&mut self, pane: PaneId) -> Arc<RwLock<KittyImageCache>> {
        self.kitty_images
            .entry(pane)
            .or_insert_with(|| Arc::new(RwLock::new(KittyImageCache::default())))
            .clone()
    }

    fn begin_kitty_image(
        &mut self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
        width: u32,
        height: u32,
        total_bytes: u32,
    ) {
        let cache = self.kitty_image_cache(pane);
        if cache.read().contains(image_id, generation) {
            self.kitty_image_assemblies
                .retain(|(target, id, _), _| *target != pane || *id != image_id);
            return;
        }
        self.kitty_image_assemblies
            .retain(|(target, id, _), _| *target != pane || *id != image_id);
        let total_bytes = usize::try_from(total_bytes).unwrap_or(usize::MAX);
        self.kitty_image_assemblies.insert(
            (pane, image_id, generation),
            KittyImageAssembly {
                width,
                height,
                total_bytes,
                bytes: Vec::with_capacity(total_bytes),
            },
        );
    }

    fn push_kitty_image_chunk(
        &mut self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
        bytes: &[u8],
    ) {
        let key = (pane, image_id, generation);
        let complete = {
            let Some(assembly) = self.kitty_image_assemblies.get_mut(&key) else {
                return;
            };
            if assembly
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_none_or(|next| next > assembly.total_bytes)
            {
                log::warn!(
                    "discarding over-declared Kitty image {image_id} generation {generation} for {pane}"
                );
                self.kitty_image_assemblies.remove(&key);
                return;
            }
            assembly.bytes.extend_from_slice(bytes);
            assembly.bytes.len() == assembly.total_bytes
        };
        if !complete {
            return;
        }
        let assembly = self
            .kitty_image_assemblies
            .remove(&key)
            .expect("completed Kitty assembly remains present");
        let cache = self.kitty_image_cache(pane);
        let retain_replaced = Arc::strong_count(&cache) > 2;
        let mut cache = cache.write();
        if cache.contains(image_id, generation) {
            return;
        }
        let expected = usize::try_from(assembly.width)
            .ok()
            .and_then(|width| {
                usize::try_from(assembly.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4));
        if expected != Some(assembly.bytes.len()) {
            log::warn!(
                "discarding malformed Kitty image {image_id} generation {generation} for {pane}"
            );
            return;
        }
        let Some(buffer) = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(
            assembly.width,
            assembly.height,
            assembly.bytes,
        ) else {
            return;
        };
        cache.insert(
            image_id,
            generation,
            Arc::new(RenderImage::new(vec![ImageFrame::new(buffer)])),
            retain_replaced,
        );
    }

    fn remove_kitty_images(&mut self, pane: PaneId, image_ids: &[u32]) {
        self.kitty_image_assemblies
            .retain(|(target, id, _), _| *target != pane || !image_ids.contains(id));
        if let Some(cache) = self.kitty_images.get(&pane) {
            let retain_removed = Arc::strong_count(cache) > 1;
            let mut cache = cache.write();
            for image_id in image_ids {
                cache.remove(*image_id, retain_removed);
            }
        }
    }

    fn clear_kitty_images(&mut self, pane: PaneId) {
        self.kitty_image_assemblies
            .retain(|(target, _, _), _| *target != pane);
        if let Some(cache) = self.kitty_images.remove(&pane) {
            cache.write().clear();
        }
    }

    fn clear_all_kitty_images(&mut self) {
        self.kitty_image_assemblies.clear();
        for cache in std::mem::take(&mut self.kitty_images).into_values() {
            cache.write().clear();
        }
    }

    fn clear_all_pasted_images(&mut self) {
        self.pane_images.clear();
        self.pasted_image_assemblies.clear();
        self.pending_pasted_image_previews.clear();
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

    pub(crate) fn pasted_image(&self, pane: PaneId, number: u32) -> Option<Arc<Image>> {
        self.pane_images
            .get(&pane)
            .and_then(|images| images.get(number))
    }

    pub(crate) fn pasted_image_revision(&self, pane: PaneId) -> u64 {
        self.pane_images
            .get(&pane)
            .map_or(0, |images| images.revision)
    }

    pub(crate) fn fetch_artifact(&mut self, request: ArtifactRequest) {
        let Some(client) = &self.attached_connection().client else {
            request.respond(Err("zz is not connected to its daemon".to_owned()));
            return;
        };
        match client.fetch_artifact(&request.token, &request.path) {
            Ok(request_id) => {
                self.artifact_fetches.insert(
                    request_id,
                    ArtifactFetch {
                        request,
                        begun: None,
                        bytes: Vec::new(),
                    },
                );
            }
            Err(error) => request.respond(Err(error.to_string())),
        }
    }

    pub(crate) fn send_artifact_reply(&self, reply: &ArtifactReply) {
        if let Some(client) = &self.attached_connection().client
            && let Err(error) = client.send_artifact_reply(&reply.token, &reply.text)
        {
            log::warn!("failed to send a page reply: {error}");
        }
    }

    fn artifact_progress(&mut self, request_id: u64) {
        let complete = self
            .artifact_fetches
            .get(&request_id)
            .and_then(|fetch| fetch.begun.as_ref().map(|begun| (begun, fetch.bytes.len())))
            .is_some_and(|(begun, received)| received >= begun.total);
        if complete && let Some(fetch) = self.artifact_fetches.remove(&request_id) {
            let begun = fetch.begun.unwrap_or_default();
            fetch.request.respond(Ok(ArtifactFile {
                mime: begun.mime,
                bytes: fetch.bytes,
                reply: begun.reply,
            }));
        }
    }

    pub(crate) fn prefetch_pasted_image(&self, pane: PaneId, number: u32) -> bool {
        let Some(client) = &self.attached_connection().client else {
            return false;
        };
        if let Err(error) = client.fetch_pasted_image(pane, number) {
            log::warn!("failed to fetch pasted image #{number} for {pane}: {error}");
            return false;
        }
        true
    }

    pub(crate) fn open_pasted_image(&mut self, pane: PaneId, number: u32, cx: &mut Context<Self>) {
        if let Some(image) = self.pasted_image(pane, number) {
            cx.emit(AttachmentPreviewRequest { image });
            return;
        }
        self.pending_pasted_image_previews.insert((pane, number));
        if !self.prefetch_pasted_image(pane, number) {
            self.pending_pasted_image_previews.remove(&(pane, number));
        }
    }

    fn begin_pasted_image(
        &mut self,
        pane: PaneId,
        number: u32,
        format: PastedImageFormat,
        total_bytes: u32,
    ) {
        let total_bytes = usize::try_from(total_bytes).unwrap_or(usize::MAX);
        self.pasted_image_assemblies.remove(&(pane, number));
        if let Some(images) = self.pane_images.get_mut(&pane) {
            images.remove(number);
        }
        self.pasted_image_assemblies.insert(
            (pane, number),
            PastedImageAssembly {
                format,
                total_bytes,
                bytes: Vec::with_capacity(total_bytes),
            },
        );
    }

    fn push_pasted_image_chunk(
        &mut self,
        pane: PaneId,
        number: u32,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) {
        let key = (pane, number);
        let complete = {
            let Some(assembly) = self.pasted_image_assemblies.get_mut(&key) else {
                return;
            };
            if assembly
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_none_or(|next| next > assembly.total_bytes)
            {
                self.pasted_image_assemblies.remove(&key);
                self.pending_pasted_image_previews.remove(&key);
                return;
            }
            assembly.bytes.extend_from_slice(bytes);
            assembly.bytes.len() == assembly.total_bytes
        };
        if !complete {
            return;
        }
        let assembly = self
            .pasted_image_assemblies
            .remove(&key)
            .expect("the completed pasted-image assembly remains present");
        let image = Arc::new(Image::from_bytes(
            ala_image_format(assembly.format),
            assembly.bytes,
        ));
        self.pane_images
            .entry(pane)
            .or_default()
            .insert(number, Arc::clone(&image));
        if self.pending_pasted_image_previews.remove(&key) {
            cx.emit(AttachmentPreviewRequest { image });
        }
    }

    fn pasted_image_unavailable(&mut self, pane: PaneId, number: u32) {
        self.pasted_image_assemblies.remove(&(pane, number));
        self.pending_pasted_image_previews.remove(&(pane, number));
        if let Some(images) = self.pane_images.get_mut(&pane) {
            images.remove(number);
        }
    }

    #[cfg(feature = "agent-pane")]
    fn apply_agent_updates(&mut self, pane: PaneId, first_seq: u64, items: Vec<Vec<u8>>) {
        if !self.watched_agents.contains(&pane) {
            return;
        }
        let cursor = self.agent_cursors.entry(pane).or_default();
        let (accepted, gapped) = accept_agent_items(pane, cursor, first_seq, items);
        if !accepted.is_empty() {
            self.agent_events.items.push((pane, accepted));
        }
        if gapped {
            if !self.agent_replays_pending.contains(&pane) {
                self.request_agent_replay_from_cursor(pane);
            }
        } else {
            self.agent_replays_pending.remove(&pane);
        }
    }

    #[cfg(not(feature = "agent-pane"))]
    fn apply_agent_updates(&mut self, _pane: PaneId, _first_seq: u64, _items: Vec<Vec<u8>>) {}

    #[cfg(feature = "agent-pane")]
    pub(crate) fn request_agent_replay_from_cursor(&mut self, pane: PaneId) -> bool {
        let from_seq = self.agent_cursors.get(&pane).copied().unwrap_or_default();
        let sent = self.send_agent_request(pane, AgentRequest::Replay { from_seq });
        if sent {
            self.agent_replays_pending.insert(pane);
        }
        sent
    }

    #[cfg(feature = "agent-pane")]
    pub(crate) fn watch_agent(&mut self, pane: PaneId, from_seq: u64) {
        self.watched_agents.insert(pane);
        if !self.agent_cursors.contains_key(&pane)
            && self.send_agent_request(pane, AgentRequest::Replay { from_seq })
        {
            self.agent_cursors.insert(pane, from_seq);
            self.agent_replays_pending.insert(pane);
        }
    }

    #[cfg(feature = "agent-pane")]
    pub(crate) fn unwatch_agent(&mut self, pane: PaneId) {
        self.watched_agents.remove(&pane);
        self.agent_cursors.remove(&pane);
        self.agent_replays_pending.remove(&pane);
        self.agent_events
            .items
            .retain(|(target, _)| *target != pane);
        self.agent_events
            .states
            .retain(|(target, _)| *target != pane);
        self.agent_events
            .sessions
            .retain(|(target, _)| *target != pane);
    }

    #[cfg(feature = "agent-pane")]
    fn clear_agent_streams(&mut self) {
        self.agent_cursors.clear();
        self.agent_replays_pending.clear();
        self.agent_events = AgentEvents::default();
    }

    #[cfg(not(feature = "agent-pane"))]
    fn clear_agent_streams(&mut self) {}

    #[cfg(feature = "agent-pane")]
    pub(crate) fn take_agent_events_for(&mut self, panes: &BTreeSet<PaneId>) -> AgentEvents {
        let pending = std::mem::take(&mut self.agent_events);
        let mut ready = AgentEvents::default();
        for (pane, items) in pending.items {
            if panes.contains(&pane) {
                ready.items.push((pane, items));
            } else {
                self.agent_events.items.push((pane, items));
            }
        }
        for (pane, state) in pending.states {
            if panes.contains(&pane) {
                ready.states.push((pane, state));
            } else {
                self.agent_events.states.push((pane, state));
            }
        }
        ready.sessions = pending
            .sessions
            .into_iter()
            .filter(|(pane, _)| panes.contains(pane))
            .collect();
        ready
    }

    #[cfg(feature = "agent-pane")]
    #[must_use]
    pub(crate) fn has_agent_events(&self) -> bool {
        !self.agent_events.is_empty()
    }

    #[cfg(feature = "agent-pane")]
    pub(crate) fn send_agent_request(&self, pane: PaneId, request: AgentRequest) -> bool {
        #[cfg(test)]
        if let Some(sink) = &self.agent_sink {
            sink.borrow_mut().push((pane, request));
            return true;
        }
        let Some(client) = self
            .connections
            .get(&self.attached_host)
            .and_then(|connection| connection.client.as_ref())
        else {
            log::trace!(target: "zz::diagnostics::mux", "agent request skipped: no client");
            return false;
        };
        let sent = match request {
            AgentRequest::Prompt { text, images } => client.agent_prompt(pane, text, images),
            AgentRequest::Cancel => client.agent_cancel(pane),
            AgentRequest::Unqueue => client.agent_unqueue(pane),
            AgentRequest::RespondPermission {
                request_id,
                option_id,
            } => client.agent_respond_permission(pane, request_id, option_id),
            AgentRequest::Replay { from_seq } => client.agent_replay(pane, from_seq),
            AgentRequest::SetConfigOption { option_id, value } => {
                client.send(&zz_protocol::ProtocolMessage::AgentSetConfigOption {
                    pane,
                    option_id,
                    value,
                })
            }
            AgentRequest::AcknowledgePromptRestore { reclaim_id } => {
                client.agent_acknowledge_prompt_restore(pane, reclaim_id)
            }
            AgentRequest::AnswerQuestion {
                request_id,
                answers,
            } => client.send(&zz_protocol::ProtocolMessage::AgentAnswerQuestion {
                pane,
                request_id,
                answers,
            }),
            AgentRequest::SessionOp(op) => client.agent_session_op(pane, op),
        };
        if let Err(error) = sent {
            log::warn!("failed to send an agent request for {pane}: {error}");
            return false;
        }
        true
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

    fn report_command_failure(
        &mut self,
        host: HostId,
        request_id: u64,
        error: &ServerError,
        cx: &mut Context<Self>,
    ) -> bool {
        if request_id == 0 {
            return false;
        }
        if self.answer(host, request_id, Err(error.to_string())) {
            cx.notify();
            return true;
        }
        let tracked = self.connections.get(&host).and_then(|connection| {
            connection.settle_command(request_id);
            connection.take_command(request_id)
        });
        match tracked {
            Some(name)
                if matches!(
                    error,
                    ServerError::PaneExited(_) | ServerError::PaneNotAttached(_)
                ) =>
            {
                log::debug!(
                    target: "zz::diagnostics::mux",
                    "command_raced_teardown host={host:?} command={name} error={error}"
                );
            }
            Some(name) => {
                log::warn!(
                    target: "zz::diagnostics::mux",
                    "command_failed host={host:?} command={name} error={error}"
                );
                Self::emit_notification(ClientMessageKind::Error, format!("{name}: {error}"), cx);
            }
            None => log::warn!(
                target: "zz::diagnostics::mux",
                "untracked_command_failed host={host:?} request_id={request_id} error={error}"
            ),
        }
        true
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

    fn finish_attach(&mut self, host: HostId, session: SessionId, cx: &mut Context<Self>) {
        self.clear_all_kitty_images();
        self.clear_all_pasted_images();
        self.clear_agent_streams();
        self.attached_snapshot_pending = false;
        let connection = self.attached_connection_mut();
        connection.last_attached_session = Some(session);
        let reconnected = connection.reconnect_attach.take().is_some();
        connection.resync_pending = false;
        connection.full_requests_pending.clear();
        connection.history.clear();
        self.error = self.error_after_next_attach.take();
        self.client_focus_attach_ready = true;
        self.client_focus_attach_pending = false;
        self.client_focus_attach_recoverable = false;
        self.sent_client_window_focus = None;
        self.flush_client_window_focus();
        if reconnected {
            let name = self
                .registry
                .get(host)
                .map_or_else(|| format!("{host:?}"), |entry| entry.name.clone());
            Self::emit_keyed_notification(
                ClientMessageKind::Success,
                format!("reconnected to {name}"),
                Some(format!("connection-{name}")),
                cx,
            );
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

    fn handle_command_response(
        &mut self,
        host: HostId,
        response: CommandResponse,
        cx: &mut Context<Self>,
    ) {
        match response {
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::MissingTarget(_) | ServerError::SessionNotFound(_),
                ..
            } if self.retry_default_after_missing_session() => {}
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::MissingTarget(_) | ServerError::SessionNotFound(_),
                ..
            } if self.core.attached_session().is_none()
                && self.core.snapshot().sessions.is_empty() =>
            {
                self.fail_client_focus_attach();
                self.error = None;
                let connection = self.attached_connection_mut();
                if !connection.resync_pending
                    && let Some(client) = &connection.client
                {
                    if let Err(error) = client.request_resync() {
                        self.error = Some(Arc::from(error.to_string()));
                    } else {
                        connection.resync_pending = true;
                    }
                }
            }
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::PaneExited(pane) | ServerError::PaneNotAttached(pane),
                ..
            } => log::debug!("ignoring stale input for detached pane {pane}"),
            CommandResponse::Error {
                request_id: 0,
                error: error @ (ServerError::MissingTarget(_) | ServerError::SessionNotFound(_)),
                ..
            } if self.client_focus_attach_pending => {
                self.fail_client_focus_attach();
                if !self.report_command_failure(host, 0, &error, cx) {
                    self.attached_connection_mut().reconnect_attach = None;
                    self.error = Some(Arc::from(error.to_string()));
                }
            }
            CommandResponse::Error {
                request_id, error, ..
            } => {
                self.printed_commands.borrow_mut().remove(&request_id);
                if !self.report_command_failure(host, request_id, &error, cx) {
                    self.attached_connection_mut().reconnect_attach = None;
                    self.error = Some(Arc::from(error.to_string()));
                }
            }
            CommandResponse::Success {
                request_id, output, ..
            } => {
                if self.printed_commands.borrow_mut().remove(&request_id) {
                    cx.emit(CommandPrinted {
                        request_id,
                        output: AsRef::<str>::as_ref(&output).to_owned(),
                    });
                }
                if self.answer(host, request_id, Ok(())) {
                    cx.notify();
                } else {
                    let connection = self.attached_connection();
                    connection.take_command(request_id);
                    if connection.settle_command(request_id) {
                        cx.notify();
                    }
                }
                self.error = None;
            }
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
