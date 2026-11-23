mod hub;
mod input;
mod messages;
mod wire;

use ala_cef::{
    BrowserEvent, BrowserGpuContext, BrowserProfilePaths, CookieImportBatch, CookieImportResult,
    ElementPickerAppearance, FrameMailbox, FrameMailboxDiagnostics, OsrFrame, PdfPrintResult,
    RuntimePhase, SessionId, SessionPhase, SiteDataClearResult, Viewport,
};
use async_channel::{Receiver, Sender};
use hub::{Hub, Page};
use messages::server;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::{
    cell::Cell,
    env, io,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone, Debug, Default)]
pub struct RuntimeConfig {
    pub scheme: &'static str,
    pub page_global: &'static str,
    pub frame_rate: Option<i32>,
    pub gpu: bool,
    pub shared_texture: bool,
    pub external_begin_frame: bool,
    pub begin_frame_adaptive: bool,
    pub remote_debugging_port: Option<u16>,
    pub ax_viewport_collapse: bool,
    pub keychain_service_name: Option<&'static str>,
    pub keychain_account_name: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub enum RuntimeSignal {
    ContextInitialized,
    RequestContextInitialized { profile: Arc<str> },
    ScheduleMessagePump(i64),
}

pub enum BrowserBootstrap {
    Runtime(BrowserRuntime),
    SubprocessExit(i32),
}

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("Ladybird Hub: {0}")]
    Io(#[from] io::Error),
    #[error("Ladybird is not ready")]
    NotReady,
    #[error("Ladybird has shut down")]
    AlreadyShutdown,
    #[error("{0} is unavailable in the Ladybird spike")]
    Unsupported(&'static str),
}

#[derive(Clone)]
pub struct BrowserMessagePump;
impl BrowserMessagePump {
    pub fn do_message_loop_work(&self) {}
}

pub struct BrowserRuntime {
    frame_rate: Option<i32>,
    paths: BrowserProfilePaths,
    phase: RuntimePhase,
    signals: (Sender<RuntimeSignal>, Receiver<RuntimeSignal>),
    hubs: HashMap<String, Arc<Hub>>,
    next_id: u64,
    count: Arc<AtomicU64>,
    background: u32,
}

pub fn bootstrap(
    config: &RuntimeConfig,
    paths: impl FnOnce() -> io::Result<BrowserProfilePaths>,
) -> Result<BrowserBootstrap, BrowserError> {
    bootstrap_with_profile_paths(config, paths()?)
}
pub fn bootstrap_with_profile_paths(
    config: &RuntimeConfig,
    paths: BrowserProfilePaths,
) -> Result<BrowserBootstrap, BrowserError> {
    Ok(BrowserBootstrap::Runtime(BrowserRuntime {
        frame_rate: config.frame_rate,
        paths,
        phase: RuntimePhase::Uninitialized,
        signals: async_channel::unbounded(),
        hubs: HashMap::new(),
        next_id: 1,
        count: Arc::default(),
        background: 0xffff_ffff,
    }))
}

impl BrowserRuntime {
    pub fn phase(&self) -> RuntimePhase {
        self.phase
    }
    pub fn message_pump(&self) -> BrowserMessagePump {
        BrowserMessagePump
    }
    pub fn do_message_loop_work(&self) {}
    pub fn signals(&self) -> Receiver<RuntimeSignal> {
        self.signals.1.clone()
    }
    pub fn profile_paths(&self) -> &BrowserProfilePaths {
        &self.paths
    }
    pub fn background_color(&self) -> u32 {
        self.background
    }
    pub fn set_background_color(&mut self, value: u32) {
        self.background = value;
    }
    pub fn set_log_file(&mut self, _: PathBuf) {}
    pub fn set_remote_debugging_port(&mut self, _: Option<u16>) {}
    pub fn shared_texture_enabled(&self) -> bool {
        false
    }
    pub fn external_begin_frame_enabled(&self) -> bool {
        false
    }
    pub fn begin_frame_adaptive_enabled(&self) -> bool {
        false
    }
    pub fn windowless_frame_rate(&self) -> i32 {
        self.frame_rate.unwrap_or(60)
    }
    pub fn frame_rate_override(&self) -> Option<i32> {
        self.frame_rate
    }
    pub fn active_session_count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
    pub fn active_data_operation_count(&self) -> u64 {
        0
    }
    pub fn start(&mut self) -> Result<(), BrowserError> {
        self.phase = RuntimePhase::Initializing;
        let _ = self.signals.0.try_send(RuntimeSignal::ContextInitialized);
        Ok(())
    }
    pub fn handle_context_initialized(&mut self) -> Result<(), BrowserError> {
        let _ = self
            .signals
            .0
            .try_send(RuntimeSignal::RequestContextInitialized {
                profile: ala_cef::DEFAULT_BROWSER_PROFILE.into(),
            });
        Ok(())
    }
    pub fn handle_request_context_initialized(&mut self, _: &str) -> Result<bool, BrowserError> {
        self.phase = RuntimePhase::Running;
        Ok(true)
    }
    pub fn ensure_profile_context(&mut self, _: &str) -> Result<bool, BrowserError> {
        Ok(true)
    }
    pub fn ensure_egress_profile_context(&mut self, _: &str) -> Result<bool, BrowserError> {
        Err(BrowserError::Unsupported("Remote browser egress"))
    }
    pub fn set_profile_proxy(&mut self, _: &str, _: u16) -> Result<(), BrowserError> {
        Err(BrowserError::Unsupported("Remote browser egress"))
    }
    pub fn import_cookies_for_session(
        &self,
        _: &BrowserSession,
        _: CookieImportBatch,
    ) -> Result<Receiver<CookieImportResult>, BrowserError> {
        Err(BrowserError::Unsupported("Cookie import"))
    }
    pub fn shutdown(&mut self) -> Result<(), BrowserError> {
        self.hubs.clear();
        self.phase = RuntimePhase::Closed;
        Ok(())
    }
    pub fn create_session(
        &mut self,
        profile: &str,
        url: &str,
        viewport: Viewport,
        zoom: f64,
        fps: Option<i32>,
        _: Option<BrowserGpuContext>,
        _: bool,
    ) -> Result<BrowserSession, BrowserError> {
        if self.phase != RuntimePhase::Running {
            return Err(BrowserError::NotReady);
        }
        if !(url.starts_with("https:")
            || url.starts_with("http:")
            || url.starts_with("about:")
            || url.starts_with("data:")
            || url.starts_with("file:"))
        {
            return Err(BrowserError::Unsupported("This URL scheme"));
        }
        let path = env::var_os("ZZ_LADYBIRD_HUB")
            .map_or_else(|| PathBuf::from("/opt/ladybird/libexec/Hub"), PathBuf::from);
        let (events, rx) = async_channel::unbounded();
        let mailbox = FrameMailbox::default();
        let id = SessionId(self.next_id);
        self.next_id += 1;
        let (profile, dir) = self
            .paths
            .ensure_profile(profile)
            .map_err(io::Error::other)?;
        let hub = match self.hubs.entry(profile.clone()) {
            std::collections::hash_map::Entry::Occupied(entry) => Arc::clone(entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                Arc::clone(entry.insert(Arc::new(Hub::launch(&path, &dir.join("ladybird"))?)))
            }
        };
        let address = Arc::new(Mutex::new(String::new()));
        let page = Page::new(
            id,
            f64::from(viewport.sanitized().scale_factor),
            fps.unwrap_or(60),
            events,
            mailbox.clone(),
            Arc::clone(&address),
            Arc::clone(&self.count),
        );
        if let Err(err) = hub.open(page) {
            self.hubs.remove(&profile);
            return Err(err.into());
        }
        let sink = BrowserCommandSink {
            hub,
            id,
            scale: Cell::new(1.0),
        };
        let mut session = BrowserSession {
            id,
            profile,
            viewport,
            zoom: Cell::new(zoom),
            phase: SessionPhase::Creating,
            events: rx,
            mailbox,
            address,
            sink,
        };
        session.set_viewport(viewport);
        session.navigate(url);
        Ok(session)
    }
}

#[derive(Clone)]
pub struct BrowserCommandSink {
    hub: Arc<Hub>,
    id: SessionId,
    scale: Cell<f32>,
}

pub struct BrowserSession {
    id: SessionId,
    profile: String,
    viewport: Viewport,
    phase: SessionPhase,
    zoom: Cell<f64>,
    events: Receiver<BrowserEvent>,
    mailbox: FrameMailbox,
    address: Arc<Mutex<String>>,
    sink: BrowserCommandSink,
}

impl Deref for BrowserSession {
    type Target = BrowserCommandSink;
    fn deref(&self) -> &Self::Target {
        &self.sink
    }
}
impl Drop for BrowserSession {
    fn drop(&mut self) {
        self.sink.hub.close(self.id);
    }
}

impl BrowserSession {
    pub fn id(&self) -> SessionId {
        self.id
    }
    pub fn phase(&self) -> SessionPhase {
        self.phase
    }
    pub fn profile(&self) -> &str {
        &self.profile
    }
    pub fn command_sink(&self) -> BrowserCommandSink {
        self.sink.clone()
    }
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }
    pub fn current_url(&self) -> Option<String> {
        Some(self.address.lock().clone())
    }
    pub fn set_viewport(&mut self, next: Viewport) {
        self.viewport = next.sanitized();
        let scale = self.viewport.scale_factor;
        self.sink.scale.set(scale);
        let mut body = Vec::new();
        for dimension in [next.width, next.height] {
            body.extend(((dimension as f32 * scale).round().clamp(1., 8192.) as i32).to_le_bytes());
        }
        body.extend(f64::from(scale * self.viewport.window_zoom).to_le_bytes());
        self.command(server::VIEW_SET_VIEWPORT, body);
        self.command(
            server::VIEW_SET_SYSTEM_VISIBILITY_STATE,
            vec![u8::from(!next.visible)],
        );
    }
    pub fn set_frame_rate(&self, fps: i32) {
        let mut body = vec![0];
        body.extend(f64::from(fps).to_le_bytes());
        body.push(0);
        self.command(server::VIEW_SET_DISPLAY_METADATA, body);
    }
    pub fn set_focus(&self, value: bool) {
        self.command(server::VIEW_SET_HAS_SYSTEM_FOCUS, vec![u8::from(value)]);
    }
    pub fn send_external_begin_frame(&self) {}
    pub fn events(&self) -> Receiver<BrowserEvent> {
        self.events.clone()
    }
    pub fn frame_mailbox_diagnostics(&self) -> FrameMailboxDiagnostics {
        self.mailbox.diagnostics()
    }
    pub fn take_frame(&self) -> Option<OsrFrame> {
        self.mailbox.take()
    }
    pub fn recycle_frame(&self, _: Vec<u8>) {}
    pub fn mark_ready(&mut self) {
        self.phase = SessionPhase::Ready;
    }
    pub fn mark_crashed(&mut self) {
        self.phase = SessionPhase::Crashed;
    }
    pub fn mark_closed(&mut self) {
        self.phase = SessionPhase::Closed;
    }
    pub fn close(&mut self, _: bool) {
        self.phase = SessionPhase::Closing;
        self.sink.hub.close(self.id);
    }
    pub fn site_connection_secure(&self) -> Option<bool> {
        None
    }
    pub fn audio_muted(&self) -> Option<bool> {
        None
    }
    pub fn take_popup(&mut self, _: SessionId) -> Option<Self> {
        None
    }
    pub fn page_zoom_factor(&self) -> f64 {
        self.zoom.get()
    }
    pub fn page_zoom_percent(&self) -> u16 {
        (self.zoom.get() * 100.).round() as u16
    }
    pub fn zoom_in(&self) -> u16 {
        self.change_zoom(
            server::VIEW_ZOOM_IN,
            ala_cef::next_page_zoom_factor(self.zoom.get(), 1),
        )
    }
    pub fn zoom_out(&self) -> u16 {
        self.change_zoom(
            server::VIEW_ZOOM_OUT,
            ala_cef::next_page_zoom_factor(self.zoom.get(), -1),
        )
    }
    pub fn reset_zoom(&self) -> u16 {
        self.change_zoom(server::VIEW_RESET_ZOOM, 1.)
    }
    fn change_zoom(&self, message: u32, value: f64) -> u16 {
        self.command(message, vec![]);
        self.zoom.set(value);
        self.page_zoom_percent()
    }
    pub fn clear_site_data(&mut self) -> Result<Receiver<SiteDataClearResult>, BrowserError> {
        Err(BrowserError::Unsupported("Clear site data"))
    }
    pub fn finish_site_data_clear(&mut self, _: i32) {}
    pub fn evaluate_number(
        &mut self,
        _: &str,
    ) -> Result<(i32, Receiver<Option<f64>>), BrowserError> {
        Err(BrowserError::Unsupported("DevTools evaluation"))
    }
    pub fn print_to_pdf(&self, _: &Path) -> Result<Receiver<PdfPrintResult>, BrowserError> {
        Err(BrowserError::Unsupported("PDF export"))
    }
    pub fn respond_file_dialog(&self, _: u64, _: Option<&[PathBuf]>) {}
    pub fn respond_js_dialog(&self, id: u64, accept: bool, text: Option<&str>) {
        let Ok(message) = u32::try_from(id) else {
            return;
        };
        let body = match message {
            server::VIEW_ALERT_CLOSED => Vec::new(),
            server::VIEW_CONFIRM_CLOSED => vec![u8::from(accept)],
            server::VIEW_PROMPT_CLOSED => {
                let mut body = vec![u8::from(accept)];
                if accept {
                    body.extend(input::utf16(text.unwrap_or_default()));
                }
                body
            }
            _ => return,
        };
        self.command(message, body);
    }
    pub fn respond_download(&self, _: u32, _: Option<&Path>) {}
    pub fn cancel_download(&self, _: u32) {}
    pub fn inspect_element_at(&self, _: i32, _: i32) {}
    pub fn toggle_dev_tools(&self) {}
    pub fn start_element_pick(&self, _: &ElementPickerAppearance) -> bool {
        false
    }
    pub fn cancel_element_pick(&self) -> bool {
        false
    }
    pub fn finish_element_capture(&self) {}
    pub fn set_ax_viewport_collapse(&self, _: bool) {}
}
