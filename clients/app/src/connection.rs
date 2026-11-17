#![cfg_attr(not(target_family = "wasm"), allow(dead_code))]

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use ala::{Context, EventEmitter};
use zz_client::agent_completion::AgentCommand;
use zz_client::agent_lens::{AgentCursor, MAX_AGENT_HISTORY_BYTES, MAX_AGENT_HISTORY_ITEMS};
use zz_client::{
    ClientCore, CoreEvent, Outbound,
    scrollback::{DeferredBackfill, HISTORY_BACKFILL_QUIET, HistoryFollowUp, HistoryPacer},
};
use zz_protocol::{
    AgentProvider, CommandInvocation, CommandRequest, CommandResponse, InputMessage, PaneId,
    PaneKindSnapshot, ProtocolMessage, ServerError, SessionId,
};

#[cfg(any(target_os = "ios", target_os = "android"))]
mod host_care;
#[cfg(any(target_os = "ios", target_os = "android"))]
pub use host_care::HostNotice;

#[cfg(any(target_os = "ios", target_os = "android"))]
fn launch_endpoint() -> Option<String> {
    std::env::var("ZZ_ALA_ENDPOINT")
        .or_else(|_| std::env::var("ZZ_SOCKET"))
        .ok()
        .map(|endpoint| endpoint.trim().to_owned())
        .filter(|endpoint| !endpoint.is_empty())
}

fn history_core() -> ClientCore {
    let mut core = ClientCore::new();
    core.retain_history();
    core
}

#[derive(Clone)]
pub struct AuthenticationPrompt {
    pub id: u64,
    pub kind: zz_protocol::gateway::AskpassPromptKind,
    pub text: String,
    pub echo: bool,
}

pub struct Connection {
    endpoint: Option<String>,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    native: Option<crate::transport::Connection>,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    client: Option<Arc<zz_daemon_client::InteractiveClient>>,
    reader: Option<ala::Task<()>>,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    auth_prompt: Option<(
        AuthenticationPrompt,
        std::sync::mpsc::Sender<zz_daemon_client::AskpassReply>,
    )>,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    auth_prompt: Option<AuthenticationPrompt>,
    resume: bool,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    pending_session: Option<String>,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    tailnet_offers: Option<u64>,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    failure: Option<zz_daemon_client::HostFailure>,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    installer: Option<crate::transport::Connection>,
    agent_setup: Option<u64>,
    retry: Option<ala::Task<()>>,
    pub core: ClientCore,
    history: HistoryPacer,
    #[cfg(test)]
    input_sink: Option<std::rc::Rc<std::cell::RefCell<Vec<ProtocolMessage>>>>,
    pub status: String,
    pub connected: bool,
    pub clock: zz_client::window_summary::DaemonClock,
    providers_requested: bool,
    pub agent_events: HashMap<PaneId, Vec<(u64, Vec<u8>)>>,
    agent_cursors: HashMap<PaneId, AgentCursor>,
    watched_agents: BTreeSet<PaneId>,
    pub flips: zz_client::agent_flip::AgentFlips,
    terminal_images: crate::terminal_images::TerminalImages,
    pasted_images: crate::terminal_images::PastedImages,
    prefix_cancel_pending: Option<u64>,
    epoch: u64,
    dialog_active: bool,
    request_id: u64,
    commands: HashMap<u64, String>,
    remembered_session: Option<SessionId>,
    attaching: bool,
    background: bool,
    retry_default: bool,
    focused: bool,
    color_scheme: Option<zz_terminal::TerminalColorScheme>,
    terminal_themes: TerminalThemes,
    link_tap: Option<(PaneId, web_time::Instant)>,
    #[cfg(target_family = "wasm")]
    socket: Option<browser::Socket>,
    #[cfg(target_family = "wasm")]
    welcome: Option<zz_protocol::Welcome>,
    #[cfg(target_family = "wasm")]
    instance: zz_protocol::ClientInstanceId,
    #[cfg(target_family = "wasm")]
    failures: u32,
    #[cfg(target_family = "wasm")]
    stale_page: bool,
}

impl EventEmitter<CoreEvent> for Connection {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundHost {
    pub endpoint: String,
    pub address: Option<std::net::IpAddr>,
    pub machine: zz_ui::compact::FoundMachine,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[must_use]
pub fn already_known(found: &FoundHost, known: &[zz_daemon_client::Endpoint]) -> bool {
    zz_daemon_client::Endpoint::parse(&found.endpoint).is_ok_and(|endpoint| {
        known.iter().any(|known| {
            known.same_machine(&endpoint)
                || found
                    .address
                    .is_some_and(|address| known.is_address(address))
        })
    })
}

pub struct FoundHosts(pub Vec<FoundHost>);

impl EventEmitter<FoundHosts> for Connection {}

pub struct OpenLink {
    pub pane: PaneId,
    pub uri: String,
    pub tapped: bool,
}

impl EventEmitter<OpenLink> for Connection {}

pub struct PreviewImage(pub Arc<ala::Image>);

impl EventEmitter<PreviewImage> for Connection {}

pub struct TerminalThemesListed;

pub struct AgentsSetUp(pub Result<String, String>);

pub const CONNECTION_LOST: &str = "The connection to the host dropped before it answered";

impl EventEmitter<AgentsSetUp> for Connection {}

impl EventEmitter<TerminalThemesListed> for Connection {}

enum TerminalThemes {
    Unasked,
    Asked(u64),
    Listed(Vec<zz_protocol::TerminalTheme>),
}

const LINK_TAP_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

impl Connection {
    pub fn expect_link_tap(&mut self, pane: PaneId) {
        self.link_tap = Some((pane, web_time::Instant::now()));
    }

    #[must_use]
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            endpoint: None,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            native: None,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            client: None,
            reader: None,
            auth_prompt: None,
            resume: false,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            pending_session: None,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            tailnet_offers: None,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            failure: None,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            installer: None,
            agent_setup: None,
            retry: None,
            core: history_core(),
            history: HistoryPacer::default(),
            #[cfg(test)]
            input_sink: None,
            status: "Connecting…".into(),
            connected: false,
            clock: zz_client::window_summary::DaemonClock::default(),
            providers_requested: false,
            agent_events: HashMap::new(),
            agent_cursors: HashMap::new(),
            watched_agents: BTreeSet::new(),
            flips: zz_client::agent_flip::AgentFlips::default(),
            terminal_images: crate::terminal_images::TerminalImages::default(),
            pasted_images: crate::terminal_images::PastedImages::default(),
            prefix_cancel_pending: None,
            epoch: 0,
            dialog_active: false,
            request_id: 1,
            commands: HashMap::new(),
            remembered_session: None,
            attaching: false,
            background: false,
            retry_default: false,
            focused: true,
            color_scheme: None,
            terminal_themes: TerminalThemes::Unasked,
            link_tap: None,
            #[cfg(target_family = "wasm")]
            socket: None,
            #[cfg(target_family = "wasm")]
            welcome: None,
            #[cfg(target_family = "wasm")]
            instance: browser::new_instance_id(),
            #[cfg(target_family = "wasm")]
            failures: 0,
            #[cfg(target_family = "wasm")]
            stale_page: false,
        }
    }

    #[must_use]
    pub fn stale_page(&self) -> bool {
        #[cfg(target_family = "wasm")]
        {
            self.stale_page
        }
        #[cfg(not(target_family = "wasm"))]
        {
            false
        }
    }

    #[must_use]
    pub fn terminal_agent(&self, pane: PaneId) -> Option<AgentProvider> {
        self.core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane))
            .and_then(zz_client::window_summary::pane_agent)
    }

    #[must_use]
    pub fn terminal_flip(&self, pane: PaneId) -> Option<zz_client::agent_flip::FlipTarget> {
        self.pane_snapshot(pane)
            .and_then(zz_client::agent_flip::flip_target)
    }

    #[must_use]
    pub fn agent_watched(&self, pane: PaneId) -> bool {
        self.watched_agents.contains(&pane)
    }

    fn watch_agent_panes(&mut self, cx: &mut Context<Self>) {
        let attached = self.core.attached_session();
        let wanted = self
            .core
            .snapshot()
            .sessions
            .iter()
            .filter(|session| Some(session.id) == attached)
            .flat_map(|session| &session.windows)
            .flat_map(|window| &window.panes)
            .filter(|(_, pane)| matches!(pane.kind, PaneKindSnapshot::Agent(_)))
            .map(|(id, _)| *id)
            .collect::<BTreeSet<_>>();
        for pane in self
            .watched_agents
            .difference(&wanted)
            .copied()
            .collect::<Vec<_>>()
        {
            self.forget_agent_stream(pane);
        }
        let added = wanted
            .difference(&self.watched_agents)
            .copied()
            .collect::<Vec<_>>();
        self.watched_agents = wanted;
        for pane in added {
            self.forget_agent_stream(pane);
            self.request_agent_replay(pane, cx);
        }
    }

    pub fn flip(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let Some(target) = self
            .pane_snapshot(pane)
            .and_then(zz_client::agent_flip::flip_target)
        else {
            return;
        };
        if !self.connected || self.core.attached_read_only() {
            return;
        }
        let mut flips = std::mem::take(&mut self.flips);
        flips.press(pane, target.to, |command| Some(self.invoke(command, cx)));
        self.flips = flips;
        cx.notify();
    }

    pub fn switch_agent_account(
        &mut self,
        pane: PaneId,
        account: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.switch_agent_account_when(pane, account, zz_client::agent_flip::FlipMode::Ask, cx);
    }

    pub fn switch_agent_account_when(
        &mut self,
        pane: PaneId,
        account: Option<&str>,
        mode: zz_client::agent_flip::FlipMode,
        cx: &mut Context<Self>,
    ) {
        if !self.connected || self.core.attached_read_only() {
            return;
        }
        let mut flips = std::mem::take(&mut self.flips);
        flips.switch_account_when(pane, account, mode, |command| {
            Some(self.invoke(command, cx))
        });
        self.flips = flips;
        cx.notify();
    }

    pub fn choose_flip(
        &mut self,
        pane: PaneId,
        mode: Option<zz_client::agent_flip::FlipMode>,
        cx: &mut Context<Self>,
    ) {
        let Some(mode) = mode else {
            self.flips.dismiss(pane);
            cx.notify();
            return;
        };
        if !self.connected || self.core.attached_read_only() {
            return;
        }
        let mut flips = std::mem::take(&mut self.flips);
        flips.choose(pane, mode, |command| Some(self.invoke(command, cx)));
        self.flips = flips;
        cx.notify();
    }

    fn pane_snapshot(&self, pane: PaneId) -> Option<&zz_protocol::PaneSnapshot> {
        self.core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane))
    }

    fn forget_agent_stream(&mut self, pane: PaneId) {
        self.agent_events.remove(&pane);
        self.agent_cursors.remove(&pane);
    }

    fn replay_agent_streams(&mut self, cx: &mut Context<Self>) {
        let watched = self.watched_agents.iter().copied().collect::<Vec<_>>();
        for pane in watched {
            self.request_agent_replay(pane, cx);
        }
    }

    #[must_use]
    pub fn agent_history_supported(&self, pane: PaneId) -> bool {
        self.agent_cursors
            .get(&pane)
            .is_some_and(|cursor| cursor.history_supported)
    }

    #[must_use]
    pub fn agent_session_load_supported(&self, pane: PaneId) -> bool {
        self.agent_cursors
            .get(&pane)
            .is_some_and(|cursor| cursor.session_load_supported)
    }

    #[must_use]
    pub fn agent_session_delete_supported(&self, pane: PaneId) -> bool {
        self.agent_cursors
            .get(&pane)
            .is_some_and(|cursor| cursor.session_delete_supported)
    }

    #[must_use]
    pub fn agent_verbs_supported(&self, pane: PaneId) -> bool {
        self.agent_cursors
            .get(&pane)
            .is_some_and(|cursor| cursor.verbs_supported)
    }

    #[must_use]
    pub fn agent_images_supported(&self, pane: PaneId) -> bool {
        self.agent_cursors
            .get(&pane)
            .is_some_and(|cursor| cursor.images_supported)
    }

    #[must_use]
    pub fn agent_watch_only(&self, pane: PaneId) -> bool {
        self.agent_cursors
            .get(&pane)
            .is_some_and(|cursor| cursor.watch)
    }

    #[must_use]
    pub fn agent_commands(&self, pane: PaneId) -> Arc<[AgentCommand]> {
        self.agent_cursors
            .get(&pane)
            .map_or_else(|| Arc::from([]), |cursor| cursor.commands.clone())
    }

    #[must_use]
    pub fn terminal_images(&self, pane: PaneId) -> Option<&crate::terminal_images::PaneImages> {
        self.terminal_images.pane(pane)
    }

    pub fn take_retired_terminal_images(&mut self) -> Vec<Arc<ala::RenderImage>> {
        self.terminal_images.take_retired()
    }

    #[must_use]
    pub fn pasted_image(&self, pane: PaneId, number: u32) -> Option<Arc<ala::Image>> {
        self.pasted_images.image(pane, number)
    }

    pub fn fetch_pasted_image(&mut self, pane: PaneId, number: u32, cx: &mut Context<Self>) {
        if self.connected && self.pasted_images.request(pane, number) {
            self.pasted_images.release_retired(cx);
            self.send(ProtocolMessage::FetchPastedImage { pane, number }, cx);
        }
    }

    pub fn preview_pasted_image(&mut self, pane: PaneId, number: u32, cx: &mut Context<Self>) {
        self.pasted_images.want_preview(pane, number);
        self.fetch_pasted_image(pane, number, cx);
        self.show_pasted_preview(cx);
    }

    fn show_pasted_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(image) = self.pasted_images.take_preview() {
            cx.emit(PreviewImage(image));
        }
    }

    #[must_use]
    pub fn uri_route(&self, pane: PaneId, uri: &str) -> zz_client::open_uri::OpenUriRoute {
        zz_client::open_uri::open_uri_route(
            self.core.snapshot(),
            self.core.attached_session(),
            pane,
            uri,
        )
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub fn navigate_browser(&mut self, pane: PaneId, url: String, cx: &mut Context<Self>) {
        let Some(PaneKindSnapshot::Browser(descriptor)) =
            self.pane_snapshot(pane).map(|pane| &pane.kind)
        else {
            return;
        };
        let mut tabs = descriptor.tabs.clone();
        let active = if descriptor.active_tab < tabs.len() {
            descriptor.active_tab
        } else {
            0
        };
        match tabs.get_mut(active) {
            Some(tab) => *tab = url,
            None => tabs.push(url),
        }
        let mut args = vec![
            "-t".into(),
            pane.to_string(),
            "-a".into(),
            active.to_string(),
            "--".into(),
        ];
        args.extend(tabs);
        self.command("set-browser-tabs", args, cx);
    }

    pub fn notify_error(&mut self, text: String, cx: &mut Context<Self>) {
        self.status.clone_from(&text);
        cx.emit(CoreEvent::ClientMessage {
            pane: None,
            kind: zz_protocol::ClientMessageKind::Error,
            text,
            duration_ms: None,
            message_id: None,
        });
        cx.notify();
    }

    pub fn notify_warning(&mut self, text: String, cx: &mut Context<Self>) {
        cx.emit(CoreEvent::ClientMessage {
            pane: None,
            kind: zz_protocol::ClientMessageKind::Warning,
            text,
            duration_ms: None,
            message_id: None,
        });
    }

    pub fn upload_image(&mut self, pane: PaneId, image: &ala::Image, cx: &mut Context<Self>) {
        let image = match zz_ui::attachment::normalize(image) {
            Ok(image) => image,
            Err(error) => {
                self.notify_warning(format!("Paste skipped: {error}"), cx);
                return;
            }
        };
        let extension = image.format.extension();
        if zz_protocol::PastedImageFormat::from_extension(extension).is_none()
            || image.bytes.is_empty()
            || image.bytes.len() > zz_protocol::MAX_PASTE_UPLOAD_BYTES as usize
        {
            self.notify_error(
                "Paste a PNG, JPEG, GIF, or WebP image of 6 MiB or less.".into(),
                cx,
            );
            return;
        }
        let upload_id = self.next_request_id();
        self.send(
            ProtocolMessage::PasteUploadBegin {
                upload_id,
                pane,
                purpose: zz_protocol::PasteUploadPurpose::PastePath,
                extension: extension.into(),
                total_bytes: image.bytes.len() as u32,
            },
            cx,
        );
        self.send_in_chunks(
            image.bytes.clone(),
            zz_protocol::MAX_PASTE_UPLOAD_CHUNK_BYTES,
            move |bytes, _, _| ProtocolMessage::PasteUploadChunk {
                upload_id,
                bytes: bytes.to_vec(),
            },
            cx,
        );
    }

    fn send_in_chunks(
        &self,
        bytes: Vec<u8>,
        chunk: usize,
        message: impl Fn(&[u8], usize, bool) -> ProtocolMessage + 'static,
        cx: &mut Context<Self>,
    ) {
        let epoch = self.epoch;
        cx.spawn(async move |this, cx| {
            let mut offset = 0;
            loop {
                let keep_running = this
                    .update(cx, |this, cx| {
                        if !this.connected || this.epoch != epoch {
                            return false;
                        }
                        #[cfg(target_family = "wasm")]
                        if this
                            .socket
                            .as_ref()
                            .is_some_and(|socket| socket.buffered_amount() > 2 * 1024 * 1024)
                        {
                            return true;
                        }
                        let end = (offset + chunk).min(bytes.len());
                        this.send(message(&bytes[offset..end], offset, end == bytes.len()), cx);
                        offset = end;
                        offset < bytes.len()
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(10))
                    .await;
            }
        })
        .detach();
    }

    fn next_request_id(&mut self) -> u64 {
        let request_id = self.request_id;
        self.request_id = self.request_id.wrapping_add(1).max(1);
        request_id
    }

    pub fn cancel_prefix(&mut self, cx: &mut Context<Self>) {
        if self.connected && self.prefix_cancel_pending.is_none() {
            let request_id = self.next_request_id();
            self.prefix_cancel_pending = Some(request_id);
            self.send(
                ProtocolMessage::Input(InputMessage::CancelPrefix { request_id }),
                cx,
            );
        }
    }

    pub fn reconcile_dialog_prefix(&mut self, active: bool, cx: &mut Context<Self>) -> bool {
        if active && !self.dialog_active {
            self.cancel_prefix(cx);
            self.dialog_active = self.connected;
        } else if !active {
            self.dialog_active = false;
        }
        self.prefix_cancel_pending.is_some()
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.endpoint = launch_endpoint();
        }
        #[cfg(target_family = "wasm")]
        {
            self.endpoint = Some(GATEWAY_LOCAL.to_owned());
        }
        self.reconnect(cx);
    }

    #[must_use]
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    #[must_use]
    pub fn browser_client(&self) -> Option<Arc<zz_daemon_client::InteractiveClient>> {
        self.connected.then(|| self.client.clone()).flatten()
    }

    #[must_use]
    pub fn busy(&self) -> bool {
        !self.connected && (self.dialing() || self.retry.is_some())
    }

    #[must_use]
    pub fn palette_state(&self) -> zz_ui::command::PaletteHostState {
        use zz_ui::command::PaletteHostState as State;
        if self.connected {
            return State::Connected {
                sessions: self.core.snapshot().sessions.len(),
            };
        }
        if self.installing() {
            return State::Installing;
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            use zz_daemon_client::HostFailure;
            match self.failure {
                Some(HostFailure::Missing) => return State::Missing,
                Some(
                    HostFailure::Incompatible { .. }
                    | HostFailure::DaemonTooOld { .. }
                    | HostFailure::ClientTooOld { .. },
                ) => return State::Incompatible,
                _ => {}
            }
        }
        match (self.busy(), self.resume) {
            (false, _) => State::Offline,
            (true, true) => State::Reconnecting,
            (true, false) => State::Connecting,
        }
    }

    #[must_use]
    pub fn installing(&self) -> bool {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.installer.is_some()
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            false
        }
    }

    fn dialing(&self) -> bool {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.native.is_some() || self.installer.is_some()
        }
        #[cfg(target_family = "wasm")]
        {
            self.socket.is_some()
        }
        #[cfg(not(any(target_family = "wasm", target_os = "ios", target_os = "android")))]
        {
            false
        }
    }

    pub fn connect_to(&mut self, endpoint: String, cx: &mut Context<Self>) {
        self.dial(endpoint, false, cx);
    }

    fn dial(&mut self, endpoint: String, background: bool, cx: &mut Context<Self>) {
        if self.endpoint.as_deref() != Some(endpoint.as_str()) {
            self.remembered_session = None;
        }
        self.endpoint = Some(endpoint);
        self.background = background;
        self.resume = false;
        #[cfg(target_family = "wasm")]
        {
            self.failures = 0;
            self.stale_page = false;
        }
        self.reconnect(cx);
    }

    pub fn connect_background(&mut self, endpoint: String, cx: &mut Context<Self>) {
        self.dial(endpoint, true, cx);
    }

    #[must_use]
    pub fn busy_or_connecting(&self) -> bool {
        #[cfg(any(target_family = "wasm", target_os = "ios", target_os = "android"))]
        {
            self.busy()
        }
        #[cfg(not(any(target_family = "wasm", target_os = "ios", target_os = "android")))]
        {
            !self.connected
        }
    }

    #[must_use]
    pub fn background(&self) -> bool {
        self.background
    }

    #[must_use]
    pub fn remembered_session(&self) -> Option<SessionId> {
        self.remembered_session.or_else(|| {
            self.core
                .snapshot()
                .sessions
                .first()
                .map(|session| session.id)
        })
    }

    pub fn send_to_background(&mut self, cx: &mut Context<Self>) {
        if self.background {
            return;
        }
        self.background = true;
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(client) = &self.client {
            client.set_loopback_discovery(false);
        }
        if self.connected {
            self.attaching = false;
            self.send(ProtocolMessage::Detach, cx);
            self.status = "Connected".into();
        }
        cx.notify();
    }

    pub fn bring_forward(&mut self, session: Option<SessionId>, cx: &mut Context<Self>) {
        let was_background = std::mem::replace(&mut self.background, false);
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(client) = &self.client {
            client.set_loopback_discovery(true);
        }
        if let Some(session) = session {
            self.remembered_session = Some(session);
        }
        if self.connected && (was_background || session.is_some()) {
            let target = self
                .remembered_session
                .map_or_else(String::new, |id| id.to_string());
            self.attach_target(target, cx);
        }
        cx.notify();
    }

    pub fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.endpoint = None;
        self.resume = false;
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.pending_session = None;
        }
        self.reconnect(cx);
    }

    #[must_use]
    pub fn client_instance_id(&self) -> Option<zz_protocol::ClientInstanceId> {
        #[cfg(target_family = "wasm")]
        {
            self.core.hello_received().then_some(self.instance)
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.client
                .as_ref()
                .map(|client| client.client_instance_id())
        }
        #[cfg(not(any(target_family = "wasm", target_os = "ios", target_os = "android")))]
        {
            None
        }
    }

    pub fn reconnect(&mut self, cx: &mut Context<Self>) {
        self.drop_agent_setup(cx);
        self.color_scheme = None;
        self.terminal_themes = TerminalThemes::Unasked;
        self.connected = false;
        self.epoch = self.epoch.wrapping_add(1);
        self.prefix_cancel_pending = None;
        self.dialog_active = false;
        self.pasted_images.clear();
        self.pasted_images.release_retired(cx);
        self.attaching = false;
        self.retry_default = false;
        self.status = "Connecting…".into();
        #[cfg(target_family = "wasm")]
        {
            self.retry = None;
            self.reader = None;
            self.socket = None;
            self.welcome = None;
            self.auth_prompt = None;
            let Some(host) = self.endpoint.clone() else {
                self.status = "Not connected".into();
                cx.notify();
                return;
            };
            match browser::Socket::connect(&host) {
                Ok((socket, receiver)) => {
                    self.socket = Some(socket);
                    self.reader = Some(cx.spawn(async move |entity, cx| {
                        while let Some(event) = receiver.recv().await {
                            if !entity
                                .update(cx, |this, cx| this.socket_event(event, cx))
                                .unwrap_or(false)
                            {
                                return;
                            }
                        }
                        let (code, reason) = receiver.close_reason();
                        let _ = entity.update(cx, |this, cx| this.closed(code, reason, cx));
                    }));
                    self.wait_for_answer(cx);
                }
                Err(error) => self.disconnected(error, cx),
            }
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.retry = None;
            self.reader = None;
            self.native = None;
            self.installer = None;
            self.failure = None;
            self.client = None;
            self.auth_prompt = None;
            self.core = history_core();
            self.history.clear();
            if let Some(endpoint) = self.endpoint.clone() {
                let target = self.pending_session.take().unwrap_or_else(|| {
                    self.remembered_session.map_or_else(
                        || std::env::var("ZZ_ALA_SESSION").unwrap_or_default(),
                        |id| id.to_string(),
                    )
                });
                self.attaching = !self.background;
                let mut native = crate::transport::Connection::connect(
                    endpoint,
                    (!self.background).then_some(target),
                    true,
                );
                let ready = native.ready.take();
                self.native = Some(native);
                self.reader = ready.map(|mut ready| {
                    cx.spawn(async move |this, cx| {
                        use futures::StreamExt as _;
                        while ready.next().await.is_some() {
                            while ready.try_recv().is_ok() {}
                            loop {
                                match this.update(cx, Connection::poll_native) {
                                    Ok(true) => {
                                        cx.background_executor()
                                            .timer(std::time::Duration::ZERO)
                                            .await;
                                    }
                                    Ok(false) => break,
                                    Err(_) => return,
                                }
                            }
                        }
                    })
                });
            } else {
                self.status = "Not connected".into();
            }
        }
        #[cfg(not(any(target_family = "wasm", target_os = "ios", target_os = "android")))]
        {
            self.status = "Open the browser client through zz-web to connect.".into();
        }
        cx.notify();
    }

    #[cfg(all(test, not(target_family = "wasm")))]
    pub(crate) fn record_input_for_test(
        &mut self,
    ) -> std::rc::Rc<std::cell::RefCell<Vec<ProtocolMessage>>> {
        let sink = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        self.input_sink = Some(std::rc::Rc::clone(&sink));
        sink
    }

    #[cfg(all(test, not(target_family = "wasm")))]
    pub(crate) fn handle_message_for_test(
        &mut self,
        message: ProtocolMessage,
        cx: &mut Context<Self>,
    ) {
        self.receive(message, cx);
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn send(&mut self, message: ProtocolMessage, cx: &mut Context<Self>) {
        let message = match message {
            ProtocolMessage::Input(InputMessage::ResizeTerminal {
                pane,
                columns,
                rows,
                cell_width_px,
                cell_height_px,
            }) => ProtocolMessage::Input(InputMessage::ResizeTerminalV2 {
                pane,
                columns,
                rows,
                cell_width_px,
                cell_height_px,
                layout_generation: self.core.layout_generation(),
            }),
            ProtocolMessage::Input(InputMessage::ClientTerminalSize { columns, rows }) => {
                ProtocolMessage::Input(InputMessage::ClientTerminalSizeV2 {
                    columns,
                    rows,
                    layout_generation: self.core.layout_generation(),
                })
            }
            message => message,
        };
        #[cfg(test)]
        if let Some(sink) = &self.input_sink {
            sink.borrow_mut().push(message);
            return;
        }
        #[cfg(target_family = "wasm")]
        {
            if let Some(capability) = message.required_capability()
                && !self
                    .welcome
                    .is_some_and(|welcome| welcome.has_capability(capability))
            {
                log::warn!("not sending a message the daemon lacks {capability} for");
                return;
            }
            if let Some(socket) = &self.socket {
                if let Err(error) = socket.send(&message) {
                    self.disconnected(error, cx);
                }
            } else {
                self.status = "Disconnected. This action was not sent.".into();
                cx.notify();
            }
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(client) = &self.client {
            match client.send(&message) {
                Ok(()) => {}
                Err(zz_daemon_client::DaemonError::DaemonLacksCapability { capability }) => {
                    log::warn!("not sending a message the daemon lacks {capability} for");
                }
                Err(error) => self.disconnect_native(error.to_string(), cx),
            }
        }
        #[cfg(not(any(target_family = "wasm", target_os = "ios", target_os = "android")))]
        let _ = (message, cx);
    }

    pub fn command(&mut self, command: &str, args: Vec<String>, cx: &mut Context<Self>) {
        self.invoke(CommandInvocation::new(command, args), cx);
    }

    pub fn providers_unrequested(&self) -> bool {
        self.connected && !self.providers_requested
    }

    pub fn request_providers(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.connected && (force || !self.providers_requested) {
            self.providers_requested = true;
            self.command("providers", Vec::new(), cx);
        }
    }

    pub fn update_provider(
        &mut self,
        provider: zz_protocol::AgentProvider,
        cx: &mut Context<Self>,
    ) {
        self.command(
            "providers",
            vec!["-u".to_owned(), provider.as_str().to_owned()],
            cx,
        );
    }

    pub fn request_agent_accounts(&mut self, cx: &mut Context<Self>) {
        if self.connected {
            self.command("agent-accounts", Vec::new(), cx);
        }
    }

    pub fn sets_up_agents(&self) -> bool {
        self.core
            .capabilities()
            .iter()
            .any(|capability| capability == zz_protocol::AGENT_SETUP_CAPABILITY)
    }

    pub fn set_up_agents(&mut self, cx: &mut Context<Self>) {
        if self.connected && self.sets_up_agents() {
            self.agent_setup =
                Some(self.invoke(CommandInvocation::new("tools", ["--install-hooks"]), cx));
        }
    }

    fn drop_agent_setup(&mut self, cx: &mut Context<Self>) {
        if self.agent_setup.take().is_some() {
            cx.emit(AgentsSetUp(Err(CONNECTION_LOST.to_owned())));
        }
    }

    pub fn signs_in(&self) -> bool {
        zz_client::agent_login::daemon_signs_in(self.core.capabilities())
    }

    pub fn start_agent_login(
        &mut self,
        provider: zz_protocol::AgentProvider,
        account: zz_client::agent_login::LoginAccount<'_>,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) {
        if !self.connected || self.core.attached_read_only() {
            return;
        }
        let command = zz_client::agent_login::entry(
            provider,
            account,
            pane,
            zz_protocol::AgentLoginBrowser::Elsewhere,
            self.core.capabilities(),
        );
        self.invoke(command, cx);
    }

    pub fn dismiss_agent_login(&mut self, login: u64, cx: &mut Context<Self>) {
        self.core.dismiss_agent_login(login);
        cx.notify();
    }

    pub fn awaits(&self, request_id: u64) -> bool {
        self.commands.contains_key(&request_id)
    }

    pub fn invoke(&mut self, command: CommandInvocation, cx: &mut Context<Self>) -> u64 {
        let request_id = self.next_request_id();
        self.commands.insert(request_id, command.name.clone());
        self.send(
            ProtocolMessage::CommandRequest(CommandRequest {
                request_id,
                command,
                prepared: false,
            }),
            cx,
        );
        request_id
    }

    pub fn attach(&mut self, session: SessionId, cx: &mut Context<Self>) {
        self.remembered_session = Some(session);
        self.attach_target(session.to_string(), cx);
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn ask_for_tailnet_hosts(&mut self, cx: &mut Context<Self>) {
        self.tailnet_offers = Some(self.invoke(CommandInvocation::new("tailnet", ["hosts"]), cx));
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn probe_tailnet_hosts(&mut self, output: &str, cx: &mut Context<Self>) {
        let offers: Vec<_> = zz_daemon_client::tailnet_offer::parse_offers(output)
            .into_iter()
            .filter(|offer| {
                !offer.os.eq_ignore_ascii_case("ios") && !offer.os.eq_ignore_ascii_case("android")
            })
            .collect();
        if offers.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let probed = offers.clone();
            let reachable = cx
                .background_executor()
                .spawn(async move { zz_daemon_client::tailnet_offer::reachable(probed) })
                .await;
            let found = offers
                .into_iter()
                .map(|offer| FoundHost {
                    endpoint: zz_daemon_client::Endpoint::Tailnet(offer.endpoint()).to_string(),
                    address: offer.address,
                    machine: zz_ui::compact::FoundMachine {
                        runs_zz: reachable.contains(&offer),
                        name: offer.name,
                        system: offer.os,
                    },
                })
                .collect();
            this.update(cx, |_, cx| cx.emit(FoundHosts(found))).ok();
        })
        .detach();
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub fn open_session(&mut self, session: String, cx: &mut Context<Self>) {
        if self.connected {
            self.attach_target(session, cx);
        } else {
            self.pending_session = Some(session);
        }
    }

    fn attach_target(&mut self, session: String, cx: &mut Context<Self>) {
        self.epoch = self.epoch.wrapping_add(1);
        self.attaching = true;
        self.send(ProtocolMessage::Attach { session }, cx);
    }

    pub fn input(&mut self, _: PaneId, input: InputMessage, cx: &mut Context<Self>) {
        self.send(ProtocolMessage::Input(input), cx);
    }

    pub fn set_color_scheme(&mut self, cx: &mut Context<Self>) {
        use penna::ActiveTheme as _;

        let color_scheme = if cx.theme().mode.is_dark() {
            zz_terminal::TerminalColorScheme::Dark
        } else {
            zz_terminal::TerminalColorScheme::Light
        };
        if self.connected
            && self.core.attached_session().is_some()
            && !self.attaching
            && self.color_scheme != Some(color_scheme)
        {
            self.color_scheme = Some(color_scheme);
            self.send(ProtocolMessage::SetColorScheme(color_scheme), cx);
        }
    }

    pub fn lists_terminal_themes(&self) -> bool {
        self.core
            .capabilities()
            .iter()
            .any(|capability| capability == zz_protocol::TERMINAL_THEMES_CAPABILITY)
    }

    pub fn edits_files(&self) -> bool {
        self.core
            .capabilities()
            .iter()
            .any(|capability| capability == zz_protocol::FILE_EDIT_CAPABILITY)
    }

    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn read_file(&mut self, pane: PaneId, path: String, cx: &mut Context<Self>) -> Option<u64> {
        if !self.connected || !self.edits_files() {
            return None;
        }
        let epoch = self.epoch;
        let request_id = self.next_request_id();
        self.send(
            ProtocolMessage::FileReadRequest {
                request_id,
                pane,
                path,
            },
            cx,
        );
        (self.connected && self.epoch == epoch).then_some(request_id)
    }

    pub fn write_file(
        &mut self,
        pane: PaneId,
        path: String,
        text: String,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        if !self.connected || !self.edits_files() {
            return None;
        }
        let request_id = self.next_request_id();
        self.send_in_chunks(
            text.into_bytes(),
            zz_protocol::MAX_FILE_WRITE_CHUNK_BYTES,
            move |bytes, offset, last| ProtocolMessage::FileWriteRequest {
                request_id,
                pane,
                path: path.clone(),
                offset: u64::try_from(offset).unwrap_or(u64::MAX),
                last,
                bytes: bytes.to_vec(),
            },
            cx,
        );
        Some(request_id)
    }

    pub fn terminal_themes(&self) -> &[zz_protocol::TerminalTheme] {
        match &self.terminal_themes {
            TerminalThemes::Listed(themes) => themes,
            TerminalThemes::Unasked | TerminalThemes::Asked(_) => &[],
        }
    }

    pub fn request_terminal_themes(&mut self, cx: &mut Context<Self>) {
        if !self.connected
            || !self.lists_terminal_themes()
            || !matches!(self.terminal_themes, TerminalThemes::Unasked)
        {
            return;
        }
        let request_id = self.next_request_id();
        self.terminal_themes = TerminalThemes::Asked(request_id);
        self.send(ProtocolMessage::TerminalThemesRequest { request_id }, cx);
    }

    fn takes_browser_screenshots(&self) -> bool {
        self.core
            .capabilities()
            .iter()
            .any(|capability| capability == zz_protocol::BROWSER_SCREENSHOT_CAPABILITY)
    }

    pub fn answer_browser_screenshot(
        &mut self,
        request_id: u64,
        png: Result<Vec<u8>, String>,
        cx: &mut Context<Self>,
    ) {
        let message = match png {
            Ok(png) if self.takes_browser_screenshots() => {
                self.send_in_chunks(
                    png,
                    zz_protocol::MAX_BROWSER_SCREENSHOT_CHUNK_BYTES,
                    move |png, _, last| ProtocolMessage::BrowserScreenshot {
                        request_id,
                        last,
                        png: png.to_vec(),
                    },
                    cx,
                );
                return;
            }
            Ok(_) => ProtocolMessage::GuiResponse(zz_protocol::GuiResponse::Error {
                request_id,
                message: "this host's zz is too old to receive browser screenshots".into(),
            }),
            Err(message) => ProtocolMessage::GuiResponse(zz_protocol::GuiResponse::Error {
                request_id,
                message,
            }),
        };
        self.send(message, cx);
    }

    pub fn set_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if focused && !self.focused && self.resume && self.native.is_none() {
            self.focused = true;
            self.reconnect(cx);
            return;
        }
        if self.focused != focused {
            self.focused = focused;
            if !focused {
                self.cancel_prefix(cx);
            }
            if self.core.attached_session().is_some() && !self.attaching {
                self.send(
                    ProtocolMessage::Input(InputMessage::ClientFocus { focused }),
                    cx,
                );
            }
        }
    }

    fn receive(&mut self, message: ProtocolMessage, cx: &mut Context<Self>) {
        self.core.handle_message(message);
        while let Some(outbound) = self.core.poll_outbound() {
            let message = match outbound {
                Outbound::RequestFull(pane) => ProtocolMessage::RequestFull { pane },
                Outbound::TreeSync => ProtocolMessage::TreeSync,
            };
            self.send(message, cx);
        }
        let mut changed = false;
        while let Some(event) = self.core.poll_event() {
            let event = match event {
                chunk @ CoreEvent::HistoryChunk { .. } => {
                    self.receive_history_chunk(chunk, cx);
                    continue;
                }
                CoreEvent::ViewportChanged { pane, damage } => {
                    self.request_history(pane, None, cx);
                    CoreEvent::ViewportChanged { pane, damage }
                }
                event => event,
            };
            self.terminal_images.apply(&event);
            changed |= !matches!(event, CoreEvent::ViewportChanged { .. });
            match &event {
                CoreEvent::HelloReceived => {
                    self.history.clear();
                    self.connected = true;
                    #[cfg(any(target_os = "ios", target_os = "android"))]
                    {
                        self.failure = None;
                    }
                    self.clock = zz_client::window_summary::DaemonClock::from_hello(
                        self.core.server_clock(),
                        crate::app::unix_seconds(),
                    );
                    self.providers_requested = false;
                    self.commands.clear();
                    #[cfg(any(target_os = "ios", target_os = "android"))]
                    self.ask_for_tailnet_hosts(cx);
                    #[cfg(target_family = "wasm")]
                    {
                        self.retry = None;
                        self.resume = true;
                        self.failures = 0;
                    }
                    self.pasted_images.clear();
                    self.agent_events.clear();
                    self.agent_cursors.clear();
                    let target = self.remembered_session.map_or_else(
                        || {
                            #[cfg(any(target_os = "ios", target_os = "android"))]
                            {
                                std::env::var("ZZ_ALA_SESSION").unwrap_or_default()
                            }
                            #[cfg(not(any(target_os = "ios", target_os = "android")))]
                            {
                                String::new()
                            }
                        },
                        |id| id.to_string(),
                    );
                    #[cfg(any(target_os = "ios", target_os = "android"))]
                    let target = self.pending_session.take().unwrap_or(target);
                    if self.background {
                        self.status = "Connected".into();
                        self.send(ProtocolMessage::Resync, cx);
                        self.replay_agent_streams(cx);
                    } else if !self.attaching {
                        self.attach_target(target, cx);
                    }
                }
                CoreEvent::Attached { session } => {
                    self.history.clear();
                    self.attaching = false;
                    self.retry_default = false;
                    self.remembered_session = Some(*session);
                    self.status = "Connected".into();
                    self.pasted_images.clear();
                    self.agent_events.clear();
                    self.agent_cursors.clear();
                    self.watched_agents.clear();
                    self.watch_agent_panes(cx);
                    self.send(
                        ProtocolMessage::Input(InputMessage::ClientFocus {
                            focused: self.focused,
                        }),
                        cx,
                    );
                }
                CoreEvent::CommandResponse(CommandResponse::Error {
                    request_id, error, ..
                }) if self.flips.owns(*request_id) => {
                    self.commands.remove(request_id);
                    let message = error.to_string();
                    if let Some(error) = self.flips.settle(*request_id, Err(&message)) {
                        self.notify_error(error, cx);
                    }
                    cx.notify();
                }
                CoreEvent::CommandResponse(CommandResponse::Error {
                    request_id, error, ..
                }) if self.agent_setup == Some(*request_id) => {
                    self.commands.remove(request_id);
                    self.agent_setup = None;
                    cx.emit(AgentsSetUp(Err(error.tmux_message())));
                }
                CoreEvent::CommandResponse(CommandResponse::Success {
                    request_id, output, ..
                }) if self.agent_setup == Some(*request_id) => {
                    self.commands.remove(request_id);
                    self.agent_setup = None;
                    cx.emit(AgentsSetUp(Ok(output.to_string())));
                }
                #[cfg(any(target_os = "ios", target_os = "android"))]
                CoreEvent::CommandResponse(CommandResponse::Error { request_id, .. })
                    if self.tailnet_offers == Some(*request_id) =>
                {
                    self.commands.remove(request_id);
                    self.tailnet_offers = None;
                }
                CoreEvent::CommandResponse(CommandResponse::Error {
                    request_id, error, ..
                }) => {
                    if *request_id == 0 && self.attaching {
                        self.attaching = false;
                        if matches!(error, ServerError::MissingTarget(_)) && !self.retry_default {
                            self.retry_default = true;
                            self.remembered_session = None;
                            self.attach_target(String::new(), cx);
                        } else {
                            self.status = error.to_string();
                        }
                    } else {
                        self.status = error.to_string();
                    }
                    match self.commands.remove(request_id) {
                        Some(_)
                            if matches!(
                                error,
                                ServerError::PaneExited(_) | ServerError::PaneNotAttached(_)
                            ) => {}
                        Some(name) => self.notify_error(format!("{name}: {error}"), cx),
                        None => {}
                    }
                }
                #[cfg(any(target_os = "ios", target_os = "android"))]
                CoreEvent::CommandResponse(CommandResponse::Success {
                    request_id, output, ..
                }) if self.tailnet_offers == Some(*request_id) => {
                    self.commands.remove(request_id);
                    self.tailnet_offers = None;
                    self.probe_tailnet_hosts(output.as_ref(), cx);
                }
                CoreEvent::CommandResponse(CommandResponse::Success { request_id, .. }) => {
                    self.commands.remove(request_id);
                    if self.flips.owns(*request_id) {
                        self.flips.settle(*request_id, Ok(()));
                        cx.notify();
                    }
                }
                CoreEvent::ClientMessage { text, .. } => self.status.clone_from(text),
                CoreEvent::Clipboard {
                    producer,
                    target,
                    text,
                    ..
                } if !text.is_empty() && self.core.clipboard_write_lands(*producer, *target) => {
                    cx.write_to_clipboard(ala::ClipboardItem::new_string(text.clone()));
                }
                CoreEvent::OpenUri { pane, uri } => {
                    let tapped = self.link_tap.take().is_some_and(|(tapped, at)| {
                        tapped == *pane && at.elapsed() < LINK_TAP_WAIT
                    });
                    cx.emit(OpenLink {
                        pane: *pane,
                        uri: uri.clone(),
                        tapped,
                    });
                }
                CoreEvent::PrefixCancelled { request_id } => {
                    if self.prefix_cancel_pending == Some(*request_id) {
                        self.prefix_cancel_pending = None;
                    }
                }
                CoreEvent::Message(message) => match message.as_ref() {
                    ProtocolMessage::PastedImageBegin {
                        pane,
                        number,
                        format,
                        total_bytes,
                    } => {
                        self.pasted_images
                            .begin(*pane, *number, *format, *total_bytes);
                        self.show_pasted_preview(cx);
                    }
                    ProtocolMessage::PastedImageChunk {
                        pane,
                        number,
                        bytes,
                    } => {
                        self.pasted_images.chunk(*pane, *number, bytes);
                        self.show_pasted_preview(cx);
                    }
                    ProtocolMessage::PastedImageUnavailable { pane, number } => {
                        self.pasted_images.unavailable(*pane, *number);
                        self.show_pasted_preview(cx);
                    }
                    ProtocolMessage::TerminalThemesResponse { request_id, themes } if matches!(self.terminal_themes, TerminalThemes::Asked(asked) if asked == *request_id) =>
                    {
                        self.terminal_themes = TerminalThemes::Listed(themes.clone());
                        cx.emit(TerminalThemesListed);
                    }
                    #[cfg(target_os = "ios")]
                    ProtocolMessage::ArtifactBegin { .. }
                    | ProtocolMessage::ArtifactChunk { .. }
                    | ProtocolMessage::ArtifactUnavailable { .. } => {
                        crate::ios_browser::artifact_message(message);
                    }
                    _ => {}
                },
                CoreEvent::SnapshotChanged => {
                    self.watch_agent_panes(cx);
                    self.flips.observe(self.core.snapshot().as_ref());
                }
                CoreEvent::AgentUpdates {
                    pane,
                    first_seq,
                    items,
                } if self.agent_watched(*pane) => {
                    let cursor = self.agent_cursors.entry(*pane).or_default();
                    let journal = self.agent_events.entry(*pane).or_default();
                    let (replay, invalid) = cursor.apply(journal, *first_seq, items);
                    let pane_trimmed =
                        cursor.trim(journal, MAX_AGENT_HISTORY_BYTES, MAX_AGENT_HISTORY_ITEMS);
                    let history_trimmed = self.trim_agent_history();
                    if pane_trimmed || history_trimmed {
                        self.status = "Showing recent agent history to limit memory use.".into();
                    }
                    if invalid {
                        self.status = "Some agent updates could not be read.".into();
                    }
                    if let Some(from_seq) = replay {
                        self.send(
                            ProtocolMessage::AgentReplay {
                                pane: *pane,
                                from_seq,
                            },
                            cx,
                        );
                    }
                }
                CoreEvent::AgentLagged { pane, .. } if self.agent_watched(*pane) => {
                    self.request_agent_replay(*pane, cx);
                }
                CoreEvent::PaneRemoved { pane } => {
                    self.history.forget(*pane);
                    self.pasted_images.remove_pane(*pane);
                    self.watched_agents.remove(pane);
                    self.forget_agent_stream(*pane);
                }
                CoreEvent::Detached { .. } if self.background => {
                    self.history.clear();
                    self.status = "Connected".into();
                }
                CoreEvent::Detached { .. } => {
                    self.history.clear();
                    self.status = "Detached".into();
                    if self.core.last_detach_was_session_destroyed() {
                        self.remembered_session = None;
                        self.attach_target(String::new(), cx);
                    }
                }
                CoreEvent::ServerStopping => self.status = "Daemon stopped".into(),
                _ => {}
            }
            self.pasted_images.release_retired(cx);
            cx.emit(event);
        }
        if changed {
            cx.notify();
        }
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn poll_native(&mut self, cx: &mut Context<Self>) -> bool {
        for _ in 0..128 {
            let Some(event) = self
                .native
                .as_ref()
                .and_then(|transport| transport.events.try_recv().ok())
            else {
                return false;
            };
            match event {
                crate::transport::Event::Connected(client) => {
                    self.resume = true;
                    client.set_loopback_discovery(!self.background);
                    let hello = client.server_hello().clone();
                    self.client = Some(client);
                    self.receive(ProtocolMessage::ServerHello(Box::new(hello)), cx);
                }
                crate::transport::Event::Message(message) => self.receive(*message, cx),
                crate::transport::Event::Prompt(prompt) => self.ask(prompt, cx),
                crate::transport::Event::Notice(notice) => {
                    self.notice(notice.kind, notice.text, cx);
                }
                crate::transport::Event::Failed(failure) => self.fail_native(failure, cx),
                crate::transport::Event::Installed(_) => {}
            }
        }
        true
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn ask(&mut self, prompt: crate::transport::Prompt, cx: &mut Context<Self>) {
        self.auth_prompt = Some((
            AuthenticationPrompt {
                id: self.next_request_id(),
                kind: prompt.kind,
                text: prompt.text,
                echo: prompt.echo,
            },
            prompt.reply,
        ));
        if !self.connected && self.installer.is_none() {
            self.status = "Authentication required".into();
        }
        cx.notify();
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    #[must_use]
    pub fn pending_auth_prompt(&self) -> Option<&AuthenticationPrompt> {
        self.auth_prompt.as_ref().map(|(prompt, _)| prompt)
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub fn answer_auth_prompt(&mut self, id: u64, answer: Option<String>, cx: &mut Context<Self>) {
        if self
            .auth_prompt
            .as_ref()
            .is_none_or(|(prompt, _)| prompt.id != id)
        {
            return;
        }
        let (_, reply) = self.auth_prompt.take().unwrap();
        let cancelled = answer.is_none();
        let answer = answer.map_or(
            zz_daemon_client::AskpassReply::Cancel,
            zz_daemon_client::AskpassReply::answer,
        );
        if self.installer.is_some() {
            let _ = reply.send(answer);
            cx.notify();
        } else if reply.send(answer).is_err() {
            self.disconnect_native("Authentication expired. Reconnect to try again.".into(), cx);
        } else if cancelled {
            self.resume = false;
            self.disconnect_native("Authentication cancelled".into(), cx);
        } else {
            self.status = "Connecting…".into();
            cx.notify();
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[must_use]
    pub fn pending_auth_prompt(&self) -> Option<&AuthenticationPrompt> {
        self.auth_prompt.as_ref()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn answer_auth_prompt(&mut self, id: u64, answer: Option<String>, cx: &mut Context<Self>) {
        if self
            .auth_prompt
            .as_ref()
            .is_none_or(|prompt| prompt.id != id)
        {
            return;
        }
        self.auth_prompt = None;
        #[cfg(target_family = "wasm")]
        {
            use zz_protocol::gateway::SignInAnswer;
            let answer = answer.map_or(SignInAnswer::Cancel, SignInAnswer::Answer);
            if matches!(answer, SignInAnswer::Cancel) {
                self.resume = false;
            }
            if self
                .socket
                .as_ref()
                .is_some_and(|socket| socket.answer(&answer).is_ok())
            {
                self.status = "Connecting…".into();
                self.wait_for_answer(cx);
            }
        }
        #[cfg(not(target_family = "wasm"))]
        drop(answer);
        cx.notify();
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn disconnect_native(&mut self, status: String, cx: &mut Context<Self>) {
        self.drop_agent_setup(cx);
        self.connected = false;
        self.client = None;
        self.native = None;
        self.auth_prompt = None;
        self.color_scheme = None;
        self.terminal_themes = TerminalThemes::Unasked;
        self.epoch = self.epoch.wrapping_add(1);
        self.prefix_cancel_pending = None;
        self.dialog_active = false;
        self.attaching = false;
        self.status = status;
        if self.resume && self.focused {
            self.retry = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.native.is_none() {
                        this.reconnect(cx);
                    }
                });
            }));
        }
        cx.notify();
    }

    pub(crate) fn request_history_prefetch(
        &mut self,
        pane: PaneId,
        target: u32,
        cx: &mut Context<Self>,
    ) {
        self.request_history(pane, Some(target), cx);
    }

    fn request_history(&mut self, pane: PaneId, prefetch: Option<u32>, cx: &mut Context<Self>) {
        if let Some((start, count)) = self.history.request(
            pane,
            prefetch,
            self.core.history_trickle_budget(),
            self.core.retained_viewport(pane),
            web_time::Instant::now(),
        ) {
            self.send(ProtocolMessage::HistoryRequest { pane, start, count }, cx);
        }
    }

    fn receive_history_chunk(&mut self, chunk: CoreEvent, cx: &mut Context<Self>) {
        let CoreEvent::HistoryChunk {
            pane,
            start,
            total,
            offset,
            columns,
            rows,
            dictionary,
        } = chunk
        else {
            return;
        };
        let mutations = self
            .core
            .retained_viewport(pane)
            .map(|retained| retained.history_mutations);
        let (apply, follow_up) = self.history.chunk_arrived(pane, mutations);
        if apply {
            self.core
                .apply_history_chunk(pane, start, total, offset, columns, rows, dictionary);
        }
        match follow_up {
            HistoryFollowUp::Prefetch(target) => self.request_history(pane, Some(target), cx),
            HistoryFollowUp::Defer(mutations) => {
                if self.history.defer(pane, mutations) {
                    self.resume_history_backfill(pane, cx);
                }
            }
            HistoryFollowUp::Backfill => self.request_history(pane, None, cx),
        }
    }

    fn resume_history_backfill(&self, pane: PaneId, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(HISTORY_BACKFILL_QUIET).await;
                let resume = this.update(cx, |connection, cx| {
                    let mutations = connection
                        .core
                        .retained_viewport(pane)
                        .map(|retained| retained.history_mutations);
                    match connection.history.resume_deferred(pane, mutations) {
                        DeferredBackfill::Request => {
                            connection.request_history(pane, None, cx);
                            false
                        }
                        DeferredBackfill::Wait => true,
                        DeferredBackfill::Done => false,
                    }
                });
                if !matches!(resume, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    fn request_agent_replay(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let from_seq = self.agent_cursors.entry(pane).or_default().replay_from();
        self.send(ProtocolMessage::AgentReplay { pane, from_seq }, cx);
    }

    fn trim_agent_history(&mut self) -> bool {
        let mut bytes: usize = self.agent_cursors.values().map(|cursor| cursor.bytes).sum();
        let mut trimmed = false;
        while bytes > MAX_AGENT_HISTORY_BYTES {
            let Some((&pane, _)) = self
                .agent_cursors
                .iter()
                .max_by_key(|(_, cursor)| cursor.bytes)
            else {
                break;
            };
            let cursor = self
                .agent_cursors
                .get_mut(&pane)
                .expect("agent cursor exists");
            let Some(journal) = self.agent_events.get_mut(&pane) else {
                break;
            };
            let old_bytes = cursor.bytes;
            let limit = old_bytes.saturating_sub(bytes - MAX_AGENT_HISTORY_BYTES);
            trimmed |= cursor.trim(journal, limit, MAX_AGENT_HISTORY_ITEMS);
            bytes -= old_bytes - cursor.bytes;
        }
        trimmed
    }

    #[cfg(target_family = "wasm")]
    fn wait_for_answer(&mut self, cx: &mut Context<Self>) {
        let answer_within = if self.endpoint.as_deref() == Some(GATEWAY_LOCAL) {
            std::time::Duration::from_secs(15)
        } else {
            REMOTE_ANSWER_WITHIN
        };
        self.retry = Some(cx.spawn(async move |entity, cx| {
            cx.background_executor().timer(answer_within).await;
            let _ = entity.update(cx, |this, cx| {
                this.disconnected("The daemon did not answer. Retrying…".into(), cx);
            });
        }));
    }

    #[cfg(target_family = "wasm")]
    fn disconnected(&mut self, reason: String, cx: &mut Context<Self>) {
        self.closed(None, reason, cx);
    }

    #[cfg(target_family = "wasm")]
    fn closed(&mut self, code: Option<u16>, reason: String, cx: &mut Context<Self>) {
        self.drop_agent_setup(cx);
        self.connected = false;
        self.epoch = self.epoch.wrapping_add(1);
        self.prefix_cancel_pending = None;
        self.dialog_active = false;
        self.pasted_images.clear();
        self.pasted_images.release_retired(cx);
        self.attaching = false;
        self.socket = None;
        self.welcome = None;
        self.reader = None;
        self.auth_prompt = None;
        if code == Some(SIGN_IN_CLOSE_CODE) {
            self.resume = false;
        }
        if code == Some(UNREACHABLE_CLOSE_CODE) {
            self.failures = self.failures.saturating_add(1);
        }
        let delay = retry_after(
            self.endpoint.as_deref() == Some(GATEWAY_LOCAL),
            self.resume,
            code,
            self.failures,
        )
        .filter(|_| !self.stale_page);
        self.status = match delay {
            Some(_) => reason,
            None => settled(&reason).to_owned(),
        };
        self.retry = delay.map(|delay| {
            cx.spawn(async move |entity, cx| {
                cx.background_executor().timer(delay).await;
                let _ = entity.update(cx, Self::reconnect);
            })
        });
        cx.notify();
    }

    #[cfg(target_family = "wasm")]
    fn prompted(&mut self, prompt: zz_protocol::gateway::SignInPrompt, cx: &mut Context<Self>) {
        use zz_protocol::gateway::AskpassPromptKind;
        let (kind, text) = match prompt.kind {
            AskpassPromptKind::Notification => (zz_protocol::ClientMessageKind::Info, prompt.text),
            AskpassPromptKind::KeyAdded => (
                zz_protocol::ClientMessageKind::Success,
                format!(
                    "Added zz's key to {}; it won't ask to sign in again",
                    prompt.text
                ),
            ),
            AskpassPromptKind::KeyFailed => (
                zz_protocol::ClientMessageKind::Warning,
                format!("Could not add zz's key: {}", prompt.text),
            ),
            _ => {
                self.retry = None;
                self.auth_prompt = Some(AuthenticationPrompt {
                    id: self.next_request_id(),
                    kind: prompt.kind,
                    text: prompt.text,
                    echo: prompt.echo,
                });
                self.status = "Authentication required".into();
                return;
            }
        };
        cx.emit(CoreEvent::ClientMessage {
            pane: None,
            kind,
            text,
            duration_ms: None,
            message_id: None,
        });
    }

    #[cfg(target_family = "wasm")]
    fn socket_event(&mut self, event: inbox::SocketEvent, cx: &mut Context<Self>) -> bool {
        match event {
            inbox::SocketEvent::Opened => {
                self.focused = web_sys::window()
                    .and_then(|window| window.document())
                    .is_some_and(|document| document.has_focus().unwrap_or(false));
                let target = (!self.background).then(|| {
                    self.remembered_session
                        .map_or_else(String::new, |id| id.to_string())
                });
                self.attaching = target.is_some();
                self.send(
                    ProtocolMessage::Hello(browser::hello(self.instance, target)),
                    cx,
                );
            }
            inbox::SocketEvent::Frame(bytes) => match zz_protocol::decode_protocol_frame(&bytes) {
                decoded if page_out_of_date(&decoded) => {
                    self.stale_page = self.endpoint.as_deref() == Some(GATEWAY_LOCAL);
                    self.disconnected(
                        if self.stale_page {
                            "This page is older than zz on the host. Reload it to update."
                        } else {
                            "This host runs a newer zz than the computer serving this page. Update zz on that computer."
                        }
                        .to_owned(),
                        cx,
                    );
                    return false;
                }
                Ok(message) if !supports_required_protocol(&message) => {
                    self.disconnected(
                        format!(
                            "The zz daemon is an older build of protocol v{} that lacks the terminal or control protocol this client needs. Restart it with zz kill-server.",
                            zz_protocol::PROTOCOL_VERSION
                        ),
                        cx,
                    );
                    return false;
                }
                Ok(message) => {
                    if let Some(revision) = daemon_below_floor(&message) {
                        self.disconnected(
                            format!(
                                "The zz daemon is revision {revision}; this client needs revision {} or newer. Restart it with zz kill-server.",
                                zz_protocol::MIN_DAEMON_REVISION
                            ),
                            cx,
                        );
                        return false;
                    }
                    if let ProtocolMessage::Welcome(welcome) = &message {
                        self.welcome = Some(*welcome);
                    }
                    self.receive(message, cx);
                }
                Err(error) => {
                    self.disconnected(format!("Connection rejected: {error}. Rebuild zz-web and the browser client together."), cx);
                    return false;
                }
            },
            inbox::SocketEvent::Focus(focused) => self.set_focused(focused, cx),
            inbox::SocketEvent::Prompt(prompt) => self.prompted(prompt, cx),
        }
        cx.notify();
        self.socket.is_some()
    }
}

#[cfg(any(target_family = "wasm", test))]
fn page_out_of_date(decoded: &Result<ProtocolMessage, zz_protocol::ProtocolError>) -> bool {
    match decoded {
        Err(zz_protocol::ProtocolError::VersionMismatch { expected, received }) => {
            received > expected
        }
        Ok(ProtocolMessage::CommandResponse(CommandResponse::Error { error, .. })) => match error {
            ServerError::ProtocolMismatch { client, server } => server > client,
            ServerError::ClientTooOld { .. } => true,
            _ => false,
        },
        _ => false,
    }
}

#[cfg(any(target_family = "wasm", test))]
fn supports_required_protocol(message: &ProtocolMessage) -> bool {
    [
        zz_protocol::PANE_FRAME_CAPABILITY,
        zz_protocol::CONTROL_CAPABILITY,
    ]
    .into_iter()
    .all(|required| match message {
        ProtocolMessage::Welcome(welcome) => welcome.has_capability(required),
        ProtocolMessage::ServerHello(hello) => hello
            .capabilities
            .iter()
            .any(|capability| capability == required),
        _ => true,
    })
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) const GATEWAY_LOCAL: &str = "local";

#[cfg(any(target_family = "wasm", test))]
const SIGN_IN_CLOSE_CODE: u16 = 4401;

#[cfg(any(target_family = "wasm", test))]
const UNREACHABLE_CLOSE_CODE: u16 = 4502;

#[cfg(target_family = "wasm")]
const REMOTE_ANSWER_WITHIN: std::time::Duration = std::time::Duration::from_secs(60);

#[cfg(any(target_family = "wasm", test))]
fn retry_after(
    local: bool,
    resumed: bool,
    code: Option<u16>,
    failures: u32,
) -> Option<std::time::Duration> {
    const RETRY: std::time::Duration = std::time::Duration::from_secs(2);
    const LONGEST: std::time::Duration = std::time::Duration::from_secs(60);
    if local {
        return Some(RETRY);
    }
    if !resumed || code == Some(SIGN_IN_CLOSE_CODE) {
        return None;
    }
    if code == Some(UNREACHABLE_CLOSE_CODE) {
        return Some(RETRY.saturating_mul(1 << failures.min(5)).min(LONGEST));
    }
    Some(RETRY)
}

#[cfg(target_family = "wasm")]
fn settled(reason: &str) -> &str {
    reason
        .trim_end_matches(" Retrying…")
        .trim_end_matches(" Reconnecting…")
}

#[cfg(target_family = "wasm")]
fn daemon_below_floor(message: &ProtocolMessage) -> Option<u32> {
    let revision = match message {
        ProtocolMessage::Welcome(welcome) => welcome.revision,
        ProtocolMessage::ServerHello(hello) => hello.revision,
        _ => return None,
    };
    (revision < zz_protocol::MIN_DAEMON_REVISION).then_some(revision)
}

#[cfg(any(target_family = "wasm", test))]
mod inbox {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    #[derive(Debug, PartialEq)]
    pub enum SocketEvent {
        Opened,
        Frame(Vec<u8>),
        Focus(bool),
        Prompt(zz_protocol::gateway::SignInPrompt),
    }

    struct State {
        bytes: Cell<usize>,
        byte_limit: usize,
        reason: RefCell<Option<(Option<u16>, String)>>,
    }

    #[derive(Clone)]
    pub struct Sender {
        sender: async_channel::Sender<SocketEvent>,
        state: Rc<State>,
    }

    pub struct Receiver {
        receiver: async_channel::Receiver<SocketEvent>,
        state: Rc<State>,
    }

    pub fn channel(capacity: usize, byte_limit: usize) -> (Sender, Receiver) {
        let (sender, receiver) = async_channel::bounded(capacity);
        let state = Rc::new(State {
            bytes: Cell::new(0),
            byte_limit,
            reason: RefCell::new(None),
        });
        (
            Sender {
                sender,
                state: state.clone(),
            },
            Receiver { receiver, state },
        )
    }

    impl Sender {
        fn accepts(&self, bytes: usize) -> bool {
            if self.sender.is_closed() {
                return false;
            }
            if self.sender.is_full()
                || bytes > self.state.byte_limit.saturating_sub(self.state.bytes.get())
            {
                self.close(
                    "Too much pending output. Reconnecting to refresh the workspace.".into(),
                );
                return false;
            }
            true
        }

        pub fn frame(&self, length: usize, copy: impl FnOnce() -> Vec<u8>) -> bool {
            self.accepts(length) && self.push(SocketEvent::Frame(copy()))
        }

        pub fn push(&self, event: SocketEvent) -> bool {
            let bytes = match &event {
                SocketEvent::Frame(bytes) => bytes.len(),
                _ => 0,
            };
            if !self.accepts(bytes) {
                return false;
            }
            if self.sender.try_send(event).is_ok() {
                self.state.bytes.set(self.state.bytes.get() + bytes);
                true
            } else {
                self.close(
                    "Too much pending output. Reconnecting to refresh the workspace.".into(),
                );
                false
            }
        }

        pub fn close(&self, reason: String) {
            self.close_with(None, reason);
        }

        pub fn close_with(&self, code: Option<u16>, reason: String) {
            self.state.reason.borrow_mut().get_or_insert((code, reason));
            self.sender.close();
        }
    }

    impl Receiver {
        pub async fn recv(&self) -> Option<SocketEvent> {
            if self.state.reason.borrow().is_some() {
                return None;
            }
            let event = self.receiver.recv().await.ok()?;
            if let SocketEvent::Frame(bytes) = &event {
                self.state
                    .bytes
                    .set(self.state.bytes.get().saturating_sub(bytes.len()));
            }
            Some(event)
        }

        pub fn close_reason(&self) -> (Option<u16>, String) {
            self.state
                .reason
                .borrow()
                .clone()
                .unwrap_or_else(|| (None, "Connection lost. Retrying…".into()))
        }
    }
}

#[cfg(target_family = "wasm")]
mod browser {
    use wasm_bindgen::{JsCast, closure::Closure};
    use zz_protocol::{
        AttachOperation, ClientHello, ClientInstanceId, ClientKind, Hello, PROTOCOL_VERSION,
        ProtocolMessage, gateway::SignInAnswer,
    };

    use super::inbox::{self, Receiver, Sender, SocketEvent};

    const CONNECTION_FAILED: &str =
        "Cannot connect to zz. Start the daemon and check the gateway socket. Retrying…";

    pub struct Socket {
        ws: web_sys::WebSocket,
        sender: Sender,
        open: Closure<dyn FnMut(web_sys::Event)>,
        message: Closure<dyn FnMut(web_sys::MessageEvent)>,
        close: Closure<dyn FnMut(web_sys::CloseEvent)>,
        error: Closure<dyn FnMut(web_sys::Event)>,
        focus: Closure<dyn FnMut(web_sys::Event)>,
        blur: Closure<dyn FnMut(web_sys::Event)>,
    }

    impl Socket {
        pub fn connect(host: &str) -> Result<(Self, Receiver), String> {
            let window = web_sys::window().ok_or("Browser window unavailable")?;
            let location = window.location();
            let scheme = if location.protocol().unwrap_or_default() == "https:" {
                "wss"
            } else {
                "ws"
            };
            let gateway = location.host().map_err(|_| "Missing gateway host")?;
            let url = if host == super::GATEWAY_LOCAL {
                format!("{scheme}://{gateway}/ws")
            } else {
                let name = String::from(js_sys::encode_uri_component(host));
                format!("{scheme}://{gateway}/ws?host={name}")
            };
            let ws = web_sys::WebSocket::new(&url).map_err(|_| "Could not open WebSocket")?;
            ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
            let (sender, receiver) = inbox::channel(64, zz_protocol::MAX_ENCODED_FRAME_BYTES);
            let tx = sender.clone();
            let open_socket = ws.clone();
            let open = Closure::new(move |_: web_sys::Event| {
                if !tx.push(SocketEvent::Opened) {
                    let _ = open_socket.close_with_code_and_reason(4008, "Client cannot keep up");
                }
            });
            ws.set_onopen(Some(open.as_ref().unchecked_ref()));
            let tx = sender.clone();
            let overflow_socket = ws.clone();
            let message = Closure::new(move |event: web_sys::MessageEvent| {
                if let Some(prompt) = event
                    .data()
                    .as_string()
                    .and_then(|text| serde_json::from_str(&text).ok())
                {
                    if !tx.push(SocketEvent::Prompt(prompt)) {
                        let _ = overflow_socket
                            .close_with_code_and_reason(4008, "Client cannot keep up");
                    }
                    return;
                }
                let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() else {
                    tx.close("Unexpected gateway message. Reconnecting…".into());
                    let _ =
                        overflow_socket.close_with_code_and_reason(4002, "Binary frames required");
                    return;
                };
                let length = buffer.byte_length() as usize;
                if length > zz_protocol::MAX_ENCODED_FRAME_BYTES {
                    tx.close("Gateway frame too large. Reconnecting…".into());
                    let _ = overflow_socket.close_with_code_and_reason(4009, "Frame too large");
                } else if !tx.frame(length, || js_sys::Uint8Array::new(&buffer).to_vec()) {
                    let _ =
                        overflow_socket.close_with_code_and_reason(4008, "Client cannot keep up");
                }
            });
            ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
            let tx = sender.clone();
            let close = Closure::new(move |event: web_sys::CloseEvent| {
                let reason = if event.reason().is_empty() {
                    "Connection lost. Retrying…".into()
                } else {
                    event.reason()
                };
                tx.close_with(Some(event.code()), reason);
            });
            ws.set_onclose(Some(close.as_ref().unchecked_ref()));
            let tx = sender.clone();
            let error = Closure::new(move |_: web_sys::Event| tx.close(CONNECTION_FAILED.into()));
            ws.set_onerror(Some(error.as_ref().unchecked_ref()));
            let focus = focus_callback(sender.clone(), ws.clone(), true);
            let blur = focus_callback(sender.clone(), ws.clone(), false);
            let socket = Self {
                ws,
                sender,
                open,
                message,
                close,
                error,
                focus,
                blur,
            };
            window
                .add_event_listener_with_callback("focus", socket.focus.as_ref().unchecked_ref())
                .map_err(|_| "Cannot watch browser focus")?;
            window
                .add_event_listener_with_callback("blur", socket.blur.as_ref().unchecked_ref())
                .map_err(|_| "Cannot watch browser focus")?;
            Ok((socket, receiver))
        }

        pub fn answer(&self, answer: &SignInAnswer) -> Result<(), ()> {
            let text = serde_json::to_string(answer).map_err(drop)?;
            self.ws.send_with_str(&text).map_err(drop)
        }

        pub fn buffered_amount(&self) -> usize {
            self.ws.buffered_amount() as usize
        }

        pub fn send(&self, message: &ProtocolMessage) -> Result<(), String> {
            if self.ws.ready_state() != web_sys::WebSocket::OPEN {
                return Err("Connection closed. This action was not sent. Reconnecting…".into());
            }
            let frame =
                zz_protocol::encode_protocol_message(message).map_err(|error| error.to_string())?;
            if (self.ws.buffered_amount() as usize).saturating_add(frame.len()) > 4 * 1024 * 1024 {
                return Err(
                    "Connection overloaded. This action was not sent. Reconnecting…".into(),
                );
            }
            self.ws
                .send_with_u8_array(&frame)
                .map_err(|_| "Could not send this action to the daemon. Reconnecting…".into())
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            self.ws.set_onopen(None);
            self.ws.set_onmessage(None);
            self.ws.set_onclose(None);
            self.ws.set_onerror(None);
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "focus",
                    self.focus.as_ref().unchecked_ref(),
                );
                let _ = window.remove_event_listener_with_callback(
                    "blur",
                    self.blur.as_ref().unchecked_ref(),
                );
            }
            let _ = self.ws.close();
            self.sender.close("Connection closed. Retrying…".into());
            let _ = (&self.open, &self.message, &self.close, &self.error);
        }
    }

    fn focus_callback(
        sender: Sender,
        socket: web_sys::WebSocket,
        focused: bool,
    ) -> Closure<dyn FnMut(web_sys::Event)> {
        Closure::new(move |_: web_sys::Event| {
            if !sender.push(SocketEvent::Focus(focused)) {
                let _ = socket.close_with_code_and_reason(4008, "Client cannot keep up");
            }
        })
    }

    pub fn new_instance_id() -> ClientInstanceId {
        ClientInstanceId(js_sys::Math::random().to_bits().max(1))
    }

    pub fn hello(instance: ClientInstanceId, target: Option<String>) -> Hello {
        let mut hello = Hello::from_client(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: instance,
            kind: ClientKind::Interactive,
            device_name: Some("Browser".into()),
            capabilities: [
                ClientHello::CLIENT_TERMINAL_CAPABILITY,
                ClientHello::CLIENT_UTF8_CAPABILITY,
                ClientHello::CLIENT_PATH_PICKER_CAPABILITY,
                zz_protocol::AGENT_LOGIN_CAPABILITY,
            ]
            .into_iter()
            .chain(zz_protocol::CLIENT_CAPABILITY_NAMES.iter().copied())
            .map(str::to_owned)
            .collect(),
            color_scheme: None,
            origin: None,
            working_directory: None,
            environment: Vec::new(),
            process_id: 0,
            revision: zz_protocol::REVISION,
        });
        hello.attach = target.map(AttachOperation::Session);
        hello
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };

    use super::{
        ProtocolMessage,
        inbox::{self, SocketEvent},
    };

    fn blob(seq: u64) -> Vec<u8> {
        format!(r#"{{"seq":{seq},"item":"promptAccepted","turn_id":1}}"#).into_bytes()
    }

    #[test]
    fn retained_hellos_require_both_control_and_terminal_capabilities() {
        for mask in 0..4 {
            let capabilities = [
                zz_protocol::PANE_FRAME_CAPABILITY,
                zz_protocol::CONTROL_CAPABILITY,
            ]
            .into_iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, capability)| capability.to_owned())
            .collect();
            let hello = ProtocolMessage::ServerHello(Box::new(zz_protocol::ServerHello {
                protocol_version: zz_protocol::PROTOCOL_VERSION,
                server_id: 1,
                client_id: zz_protocol::ClientId(1),
                client_instance_id: zz_protocol::ClientInstanceId(1),
                capabilities,
                appearance: zz_terminal::TerminalAppearance::default(),
                appearance_provenance: zz_terminal::AppearanceProvenance::default(),
                mux_options: zz_protocol::MuxOptions::default(),
                status: zz_protocol::StatusLine::default(),
                key_tables: Vec::new(),
                clock: 0,
                platform: zz_protocol::HostPlatform::default(),
                revision: zz_protocol::REVISION,
            }));
            let encoded = zz_protocol::encode_protocol_message(&hello).unwrap();
            let decoded = zz_protocol::decode_protocol_frame(&encoded).unwrap();
            assert_eq!(super::supports_required_protocol(&decoded), mask == 3);
        }
        assert!(super::supports_required_protocol(
            &ProtocolMessage::TreeSync
        ));
    }

    #[test]
    fn only_a_newer_host_marks_the_web_page_out_of_date() {
        use zz_protocol::{CommandResponse, ProtocolError, ServerError};
        let mismatch = |received| {
            Err(ProtocolError::VersionMismatch {
                expected: 7,
                received,
            })
        };
        let refused = |error| {
            Ok(ProtocolMessage::CommandResponse(CommandResponse::Error {
                request_id: 0,
                error,
                output: zz_protocol::RawText::default(),
            }))
        };
        assert!(super::page_out_of_date(&mismatch(8)));
        assert!(!super::page_out_of_date(&mismatch(6)));
        assert!(super::page_out_of_date(&refused(
            ServerError::ClientTooOld {
                client: 3,
                minimum: 4
            }
        )));
        assert!(super::page_out_of_date(&refused(
            ServerError::ProtocolMismatch {
                client: 7,
                server: 8
            }
        )));
        assert!(!super::page_out_of_date(&refused(
            ServerError::ProtocolMismatch {
                client: 8,
                server: 7
            }
        )));
        assert!(!super::page_out_of_date(&Ok(ProtocolMessage::TreeSync)));
    }

    fn ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("expected a ready queue operation"),
        }
    }

    #[test]
    fn queue_releases_byte_budget_when_frames_are_consumed() {
        let (sender, receiver) = inbox::channel(4, 10);
        assert!(sender.push(SocketEvent::Opened));
        assert!(sender.frame(6, || vec![0; 6]));
        assert!(sender.frame(4, || vec![1; 4]));
        assert_eq!(ready(receiver.recv()), Some(SocketEvent::Opened));
        assert_eq!(ready(receiver.recv()), Some(SocketEvent::Frame(vec![0; 6])));
        assert!(sender.frame(6, || vec![2; 6]));
        assert_eq!(ready(receiver.recv()), Some(SocketEvent::Frame(vec![1; 4])));
        assert_eq!(ready(receiver.recv()), Some(SocketEvent::Frame(vec![2; 6])));
        assert!(sender.push(SocketEvent::Focus(false)));
        assert_eq!(ready(receiver.recv()), Some(SocketEvent::Focus(false)));
    }

    #[test]
    fn queue_overflow_closes_without_copying_the_extra_frame() {
        let (sender, receiver) = inbox::channel(4, 10);
        assert!(sender.frame(8, || vec![0; 8]));
        assert!(!sender.frame(3, || panic!("overflow must be rejected before copying")));
        assert_eq!(ready(receiver.recv()), None);
        assert!(
            receiver
                .close_reason()
                .1
                .contains("Too much pending output")
        );
        sender.close("Later close event".into());
        assert!(
            receiver
                .close_reason()
                .1
                .contains("Too much pending output")
        );
    }

    #[test]
    fn a_full_event_queue_cannot_lose_its_close_reason() {
        let (sender, receiver) = inbox::channel(1, 10);
        assert!(sender.push(SocketEvent::Opened));
        sender.close("Daemon stopped".into());
        assert_eq!(ready(receiver.recv()), None);
        assert_eq!(receiver.close_reason(), (None, "Daemon stopped".to_owned()));
        assert!(!sender.push(SocketEvent::Focus(true)));
    }

    #[test]
    fn gateway_hosts_wait_for_sign_in_and_back_off_when_unreachable() {
        use super::retry_after;
        use std::time::Duration;

        let seconds = |seconds| Some(Duration::from_secs(seconds));
        assert_eq!(retry_after(true, false, Some(4502), 9), seconds(2));
        assert_eq!(retry_after(false, false, Some(4502), 1), None);
        assert_eq!(retry_after(false, false, None, 0), None);
        assert_eq!(retry_after(false, true, Some(4401), 1), None);
        assert_eq!(retry_after(false, true, Some(1006), 0), seconds(2));
        let backoff: Vec<_> = (1..=7)
            .map(|failures| retry_after(false, true, Some(4502), failures))
            .collect();
        assert_eq!(backoff, [4, 8, 16, 32, 60, 60, 60].map(seconds));
    }

    #[test]
    fn event_capacity_overflow_also_closes_the_connection() {
        let (sender, receiver) = inbox::channel(1, 10);
        assert!(sender.push(SocketEvent::Focus(true)));
        assert!(!sender.frame(8, || panic!("a full queue must not copy another frame")));
        assert_eq!(ready(receiver.recv()), None);
        assert!(
            receiver
                .close_reason()
                .1
                .contains("Too much pending output")
        );
    }

    #[cfg(not(target_family = "wasm"))]
    #[ala::test]
    fn viewport_frames_leave_the_connection_unnotified(cx: &mut ala::TestAppContext) {
        use ala::AppContext as _;
        use zz_protocol::{ClientView, Event, EventPayload, PaneId, SessionId};

        let connection = cx.new(super::Connection::new);
        let notified = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = std::rc::Rc::clone(&notified);
        let _observer =
            cx.update(|cx| cx.observe(&connection, move |_, _| count.set(count.get() + 1)));
        let event = |sequence, payload| ProtocolMessage::Event(Event { sequence, payload });
        connection.update(cx, |connection, cx| {
            connection.handle_message_for_test(
                event(
                    1,
                    EventPayload::TerminalViewport {
                        pane: PaneId(1),
                        viewport: zz_terminal::TerminalViewport::blank(
                            80,
                            24,
                            zz_terminal::SessionStatus::Running,
                        ),
                    },
                ),
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(notified.get(), 0);
        connection.update(cx, |connection, cx| {
            connection.handle_message_for_test(
                event(
                    2,
                    EventPayload::ClientView(ClientView {
                        session: Some(SessionId(1)),
                        layout_generation: 1,
                        ..Default::default()
                    }),
                ),
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(notified.get(), 1);
    }

    #[cfg(not(target_family = "wasm"))]
    #[ala::test]
    fn an_agent_pane_is_watched_and_flips_through_a_busy_choice(cx: &mut ala::TestAppContext) {
        use ala::AppContext as _;
        use zz_client::agent_flip::{Flip, FlipMode};
        use zz_protocol::{
            AgentDescriptor, AgentProvider, CommandResponse, Event, EventPayload, LayoutNode,
            MuxSnapshot, PaneId, PaneKindSnapshot, PaneSnapshot, PaneSummary, ServerError,
            SessionId, SessionSnapshot, WindowId, WindowSnapshot,
        };

        let pane = PaneId(1);
        let snapshot = |generation, kind: PaneKindSnapshot| MuxSnapshot {
            generation,
            sessions: vec![SessionSnapshot {
                id: SessionId(1),
                name: "test".into(),
                active_window: WindowId(1),
                viewers: Vec::new(),
                windows: vec![WindowSnapshot {
                    id: WindowId(1),
                    index: 0,
                    name: "test".into(),
                    automatic_rename: true,
                    active_pane: pane,
                    zoomed_pane: None,
                    layout: LayoutNode::Pane(pane),
                    panes: [(
                        pane,
                        PaneSnapshot {
                            id: pane,
                            title: "test".into(),
                            kind,
                            synchronized_input: false,
                            bell: false,
                            dead: false,
                            dead_status: None,
                            border_colour: None,
                            active_border_colour: None,
                            border_status_text: String::new(),
                            mode: None,
                            status: None,
                            summary: PaneSummary {
                                process: vec!["claude".to_owned()],
                                ..PaneSummary::default()
                            },
                        },
                    )]
                    .into(),
                    layout_dump: String::new(),
                    visible_layout_dump: String::new(),
                    status_label: String::new(),
                    activity: false,
                    silence: false,
                    pane_border_status: zz_protocol::PaneBorderStatus::default(),
                    pane_border_lines: zz_protocol::PaneBorderLines::default(),
                    pane_border_indicators: zz_protocol::PaneBorderIndicators::default(),
                    pane_order: vec![pane],
                    pane_z_order: vec![pane],
                    floating: Vec::new(),
                    modal: None,
                    sx: 0,
                    sy: 0,
                    pane_base_index: 0,
                }],
            }],
            focused_window: Some(WindowId(1)),
        };
        let event = |sequence, payload| ProtocolMessage::Event(Event { sequence, payload });
        let updates = |seq| EventPayload::AgentUpdates {
            pane,
            first_seq: seq,
            items: vec![blob(seq)],
        };
        let chat = || {
            PaneKindSnapshot::Agent(AgentDescriptor {
                provider: AgentProvider::ClaudeCode,
                ..AgentDescriptor::default()
            })
        };
        let connection = cx.new(super::Connection::new);
        let sent = connection.update(cx, |connection, _| connection.record_input_for_test());
        let replays = || {
            sent.borrow()
                .iter()
                .filter(|message| {
                    matches!(
                        message,
                        ProtocolMessage::AgentReplay { pane: target, from_seq: 0 } if *target == pane
                    )
                })
                .count()
        };
        let flips = || {
            sent.borrow()
                .iter()
                .filter_map(|message| match message {
                    ProtocolMessage::CommandRequest(request)
                        if request.command.name == "flip-agent" =>
                    {
                        Some((
                            request.request_id,
                            request
                                .command
                                .args
                                .iter()
                                .map(|arg| arg.as_str().to_owned())
                                .collect::<Vec<_>>(),
                        ))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let reply = |request_id, error: Option<&str>| {
            ProtocolMessage::CommandResponse(match error {
                Some(error) => CommandResponse::Error {
                    request_id,
                    error: ServerError::InvalidCommand(error.to_owned()),
                    output: zz_protocol::RawText::default(),
                },
                None => CommandResponse::Success {
                    request_id,
                    output: zz_protocol::RawText::default(),
                    exit_code: 0,
                    stderr: String::new(),
                    stdout_claim: zz_protocol::StdoutClaim::default(),
                },
            })
        };
        connection.update(cx, |connection, cx| {
            connection.connected = true;
            connection.handle_message_for_test(
                ProtocolMessage::Attached {
                    session: SessionId(1),
                    snapshot: snapshot(1, PaneKindSnapshot::Terminal),
                    read_only: false,
                    client_flags: String::new(),
                },
                cx,
            );
            assert_eq!(
                connection.terminal_agent(pane),
                Some(AgentProvider::ClaudeCode)
            );
            assert!(!connection.agent_watched(pane));
            connection.handle_message_for_test(event(2, updates(1)), cx);
            assert!(!connection.agent_events.contains_key(&pane));

            connection.flip(pane, cx);
            let (busy, args) = flips()[0].clone();
            assert_eq!(args, ["-t", "%1"]);
            connection.handle_message_for_test(reply(busy, Some("busy: a turn")), cx);
            assert_eq!(
                connection.flips.state(pane),
                &Flip::Busy("a turn".to_owned())
            );
            connection.choose_flip(pane, Some(FlipMode::WhenIdle), cx);
            let (armed, args) = flips()[1].clone();
            assert_eq!(args, ["-t", "%1", "--when-idle"]);
            connection.handle_message_for_test(reply(armed, None), cx);
            assert_eq!(connection.flips.state(pane), &Flip::Armed);

            connection
                .handle_message_for_test(event(3, EventPayload::Snapshot(snapshot(2, chat()))), cx);
            assert_eq!(connection.flips.state(pane), &Flip::Idle);
            assert!(connection.agent_watched(pane));
            assert_eq!(replays(), 1);
            connection.handle_message_for_test(event(4, updates(1)), cx);
            assert_eq!(connection.agent_events[&pane].len(), 1);

            connection.handle_message_for_test(
                event(
                    5,
                    EventPayload::Snapshot(snapshot(3, PaneKindSnapshot::Terminal)),
                ),
                cx,
            );
            assert!(!connection.agent_watched(pane));
            assert!(!connection.agent_events.contains_key(&pane));
            connection.handle_message_for_test(event(6, updates(2)), cx);
            assert!(!connection.agent_events.contains_key(&pane));
        });
    }
}
