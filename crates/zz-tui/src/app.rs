use std::{
    collections::{HashMap, HashSet},
    mem,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU8, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use zz_client::{ClientCore, CoreEvent, InputEvent, Outbound, PrefixView};
use zz_daemon_client::{Endpoint, HostEntry, InteractiveClient};
use zz_protocol::{
    BrowserCommand, BrowserDescriptor, ClientExitAction, CommandInvocation, CommandResponse,
    GuiResponse, InputMessage, NEW_SESSION_ATTACH_CAPABILITY, PaneId, PaneKindSnapshot,
    ProtocolMessage, ServerError, ServerHello, TerminalUiCommand,
};
use zz_terminal::{SearchQuery, TerminalViewAction, TerminalViewport};

use crate::{
    browser::{BrowserFrameProvider, BrowserState, BrowserSurface, BrowserWait, SurfaceChanges},
    clipboard::{self, Osc52},
    input::{self, InputOutcome},
    kitty::{
        FILE_PROBE_IMAGE_ID, FrameTransport, KittyImageAssembler, KittyImageData, PROBE_IMAGE_ID,
    },
    render::{FrameDamage, Renderer, merge_damage},
    state::{ClientMessage, HostSwitch, Model},
    terminal_event::{Event as TerminalEvent, EventParser},
    tty::{MouseArming, TerminalGuard, TerminalOptions, TerminalSize},
};

#[cfg(unix)]
mod event_loop;
#[cfg(unix)]
use event_loop::EventLoop;

enum MainEvent {
    Core {
        connection: u64,
        event: Box<CoreEvent>,
    },
    Frames(u64),
    KittyImages(u64),
    ClipboardQuery(u64),
    Terminal(Result<TerminalEvent, String>),
    Disconnected {
        connection: u64,
        error: String,
    },
    Resize,
    Signal,
    Suspend,
    Resume,
    Repaint,
}

const KITTY_GATE_PROBING: u8 = 0;
const KITTY_GATE_ENABLED: u8 = 1;
const KITTY_GATE_DISABLED: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KittyProbeState {
    Idle,
    Probing,
    Enabled,
    Disabled,
}

struct KittyProbe {
    state: KittyProbeState,
    file_pending: bool,
    transport_override: Option<FrameTransport>,
    transport: FrameTransport,
}

impl KittyProbe {
    const fn new(transport_override: Option<FrameTransport>, probing: bool) -> Self {
        Self {
            state: if probing {
                KittyProbeState::Probing
            } else {
                KittyProbeState::Idle
            },
            file_pending: probing,
            transport_override,
            transport: match transport_override {
                Some(transport) => transport,
                None => FrameTransport::Inline,
            },
        }
    }

    const fn transport(&self) -> FrameTransport {
        self.transport
    }

    fn start(&mut self) -> bool {
        if self.state != KittyProbeState::Idle {
            return false;
        }
        self.state = KittyProbeState::Probing;
        self.file_pending = true;
        true
    }

    fn observe(&mut self, event: &TerminalEvent) -> KittyProbeUpdate {
        let mut update = KittyProbeUpdate::default();
        match event {
            TerminalEvent::KittyGraphicsResponse { image_id, ok }
                if *image_id == PROBE_IMAGE_ID =>
            {
                update.consumed = true;
                if self.state == KittyProbeState::Probing {
                    self.state = if *ok {
                        KittyProbeState::Enabled
                    } else {
                        KittyProbeState::Disabled
                    };
                    update.graphics = Some(*ok);
                }
            }
            TerminalEvent::KittyGraphicsResponse { image_id, ok }
                if *image_id == FILE_PROBE_IMAGE_ID =>
            {
                update.consumed = true;
                self.resolve_file_probe(*ok, &mut update);
            }
            TerminalEvent::DeviceAttributes => {
                update.consumed = true;
                if self.state == KittyProbeState::Probing {
                    self.state = KittyProbeState::Disabled;
                    update.graphics = Some(false);
                }
                self.resolve_file_probe(false, &mut update);
            }
            _ => {}
        }
        update
    }

    fn resolve_file_probe(&mut self, ok: bool, update: &mut KittyProbeUpdate) {
        if !self.file_pending {
            return;
        }
        self.file_pending = false;
        update.finish_file_probe = true;
        let resolved = resolve_frame_transport(ok, self.transport_override);
        if resolved != self.transport {
            self.transport = resolved;
            update.transport = Some(resolved);
        }
    }
}

#[derive(Default)]
struct KittyProbeUpdate {
    consumed: bool,
    graphics: Option<bool>,
    transport: Option<FrameTransport>,
    finish_file_probe: bool,
}

const fn resolve_frame_transport(
    file_supported: bool,
    transport_override: Option<FrameTransport>,
) -> FrameTransport {
    match transport_override {
        Some(transport) => transport,
        None if file_supported => FrameTransport::File,
        None => FrameTransport::Inline,
    }
}

fn configured_frame_transport_override() -> Option<FrameTransport> {
    let value = std::env::var("ZZ_TUI_FRAMES").ok()?;
    let transport = parse_frame_transport_override(&value);
    if transport.is_none() {
        log::warn!("ignoring invalid ZZ_TUI_FRAMES value {value:?}; expected file or inline");
    }
    transport
}

fn parse_frame_transport_override(value: &str) -> Option<FrameTransport> {
    match value.trim().to_ascii_lowercase().as_str() {
        "file" => Some(FrameTransport::File),
        "inline" => Some(FrameTransport::Inline),
        _ => None,
    }
}

#[derive(Default)]
struct FrameState {
    pending: HashMap<PaneId, PendingFrame, foldhash::fast::FixedState>,
    wake_pending: bool,
}

struct PendingFrame {
    viewport: TerminalViewport,
    damage: FrameDamage,
}

#[derive(Default)]
struct FrameInbox(Mutex<FrameState>);

impl FrameInbox {
    fn publish(
        &self,
        pane: PaneId,
        viewport: TerminalViewport,
        damage: FrameDamage,
        connection: u64,
        events: &mpsc::Sender<MainEvent>,
    ) {
        let should_wake = {
            let mut state = self.0.lock().expect("frame inbox poisoned");
            state
                .pending
                .entry(pane)
                .and_modify(|pending| {
                    pending.viewport.clone_from(&viewport);
                    merge_damage(&mut pending.damage, damage.clone());
                })
                .or_insert(PendingFrame { viewport, damage });
            if state.wake_pending {
                false
            } else {
                state.wake_pending = true;
                true
            }
        };
        if should_wake {
            let _ = events.send(MainEvent::Frames(connection));
        }
    }

    fn take(&self) -> HashMap<PaneId, PendingFrame, foldhash::fast::FixedState> {
        let mut state = self.0.lock().expect("frame inbox poisoned");
        state.wake_pending = false;
        mem::take(&mut state.pending)
    }

    fn recycle(&self, mut completed: HashMap<PaneId, PendingFrame, foldhash::fast::FixedState>) {
        debug_assert!(completed.is_empty());
        let mut state = self.0.lock().expect("frame inbox poisoned");
        if completed.capacity() > state.pending.capacity() {
            completed.extend(state.pending.drain());
            state.pending = completed;
        }
    }

    fn clear(&self) {
        let mut state = self.0.lock().expect("frame inbox poisoned");
        state.pending.clear();
        state.wake_pending = false;
    }

    fn remove(&self, pane: PaneId) {
        self.0
            .lock()
            .expect("frame inbox poisoned")
            .pending
            .remove(&pane);
    }
}

enum KittyImageUpdate {
    Reset,
    Ready(KittyImageData),
    Removed { pane: PaneId, image_ids: Vec<u32> },
}

#[derive(Default)]
struct KittyImageState {
    assembler: KittyImageAssembler,
    pending: Vec<KittyImageUpdate>,
    wake_pending: bool,
}

#[derive(Default)]
struct KittyImageInbox(Mutex<KittyImageState>);

impl KittyImageInbox {
    fn begin(
        &self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
        width: u32,
        height: u32,
        total_bytes: u32,
    ) {
        self.0
            .lock()
            .expect("Kitty image inbox poisoned")
            .assembler
            .begin(pane, image_id, generation, width, height, total_bytes);
    }

    fn push_chunk(
        &self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
        bytes: Vec<u8>,
        connection: u64,
        events: &mpsc::Sender<MainEvent>,
    ) {
        let should_wake = {
            let mut state = self.0.lock().expect("Kitty image inbox poisoned");
            let Some(image) = state
                .assembler
                .push_chunk(pane, image_id, generation, bytes)
            else {
                return;
            };
            state.pending.push(KittyImageUpdate::Ready(image));
            if state.wake_pending {
                false
            } else {
                state.wake_pending = true;
                true
            }
        };
        if should_wake {
            let _ = events.send(MainEvent::KittyImages(connection));
        }
    }

    fn remove(
        &self,
        pane: PaneId,
        image_ids: Vec<u32>,
        connection: u64,
        events: &mpsc::Sender<MainEvent>,
    ) {
        if image_ids.is_empty() {
            return;
        }
        let should_wake = {
            let mut state = self.0.lock().expect("Kitty image inbox poisoned");
            state.assembler.remove(pane, &image_ids);
            state
                .pending
                .push(KittyImageUpdate::Removed { pane, image_ids });
            if state.wake_pending {
                false
            } else {
                state.wake_pending = true;
                true
            }
        };
        if should_wake {
            let _ = events.send(MainEvent::KittyImages(connection));
        }
    }

    fn remove_pane(&self, pane: PaneId) {
        let mut state = self.0.lock().expect("Kitty image inbox poisoned");
        state.assembler.remove_pane(pane);
        state.pending.retain(|update| match update {
            KittyImageUpdate::Reset => true,
            KittyImageUpdate::Ready(image) => image.pane != pane,
            KittyImageUpdate::Removed { pane: target, .. } => *target != pane,
        });
    }

    fn take(&self) -> Vec<KittyImageUpdate> {
        let mut state = self.0.lock().expect("Kitty image inbox poisoned");
        state.wake_pending = false;
        mem::take(&mut state.pending)
    }

    fn clear(&self) {
        let mut state = self.0.lock().expect("Kitty image inbox poisoned");
        state.assembler.clear();
        state.pending.clear();
        state.wake_pending = false;
    }

    fn reset_attachment(&self, connection: u64, events: &mpsc::Sender<MainEvent>) {
        let should_wake = {
            let mut state = self.0.lock().expect("Kitty image inbox poisoned");
            state.assembler.clear();
            state.pending.clear();
            state.pending.push(KittyImageUpdate::Reset);
            if state.wake_pending {
                false
            } else {
                state.wake_pending = true;
                true
            }
        };
        if should_wake {
            let _ = events.send(MainEvent::KittyImages(connection));
        }
    }
}

fn apply_kitty_updates(
    renderer: &mut Renderer,
    updates: Vec<KittyImageUpdate>,
    accept_images: bool,
) {
    for update in updates {
        match update {
            KittyImageUpdate::Reset => renderer.reset_kitty_images(),
            KittyImageUpdate::Ready(image) if accept_images => renderer.install_kitty_image(image),
            KittyImageUpdate::Removed { pane, image_ids } if accept_images => {
                renderer.remove_kitty_images(pane, &image_ids);
            }
            _ => {}
        }
    }
}

struct PreparedConnection {
    client: Arc<InteractiveClient>,
    core: Arc<Mutex<ClientCore>>,
}

enum HostSwitchDecision<T> {
    Current,
    Switch { host: HostSwitch, connected: T },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachAttempt {
    Idle,
    Default,
    Explicit,
    Remembered,
}

impl AttachAttempt {
    const fn is_pending(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

fn attach_attempt_owns_missing_response(attempt: AttachAttempt, error: &ServerError) -> bool {
    attempt.is_pending()
        && matches!(
            error,
            ServerError::MissingTarget(_) | ServerError::SessionNotFound(_)
        )
}

enum ProtocolOutcome {
    None,
    Repaint,
    RepaintAll,
    QueueControl(Vec<u8>),
    Exit(TuiExit),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum PendingPaint {
    None,
    Frames,
    Repaint,
    RepaintAll,
}

const MAX_COALESCED_EVENTS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
enum TuiExit {
    Detached(String),
    DetachedHangup(String),
    Exec { command: String, shell: String },
    Exited,
    ServerExited,
    ServerExitedUnexpectedly,
}

impl TuiExit {
    fn notice(&self) -> String {
        match self {
            Self::Detached(session) => format!("[detached (from session {session})]"),
            Self::DetachedHangup(session) => {
                format!("[detached and SIGHUP (from session {session})]")
            }
            Self::Exec { .. } => String::new(),
            Self::Exited => "[exited]".to_owned(),
            Self::ServerExited => "[server exited]".to_owned(),
            Self::ServerExitedUnexpectedly => "[server exited unexpectedly]".to_owned(),
        }
    }

    const fn exit_code(&self) -> u8 {
        match self {
            Self::Detached(_) | Self::DetachedHangup(_) | Self::Exec { .. } | Self::Exited => 0,
            Self::ServerExited | Self::ServerExitedUnexpectedly => 1,
        }
    }
}

pub(crate) enum InitialAttach {
    Connected { messages: Vec<ProtocolMessage> },
}

#[cfg(unix)]
pub(crate) fn run(
    initial: InteractiveClient,
    mut endpoint: Endpoint,
    local_endpoint: Endpoint,
    initial_attach: InitialAttach,
    terminal_options: Option<TerminalOptions>,
    host_label: String,
    local_host_label: String,
    fleet_hosts: Vec<HostEntry>,
    browser_provider: Option<Box<dyn BrowserFrameProvider>>,
) -> Result<(), String> {
    let InitialAttach::Connected {
        messages: initial_messages,
    } = initial_attach;
    let mut read_only = false;
    let mut client_flags = None;
    let attempt = AttachAttempt::Explicit;
    let size = TerminalSize::detect().map_err(|error| error.to_string())?;
    let mut core = seeded_core(initial.server_hello().clone());
    let TerminalOptions {
        extended_keys,
        focus_events,
        clear_on_attach,
    } = terminal_options.unwrap_or_default();
    let mut client = Arc::new(initial);
    let escape_time = Arc::new(AtomicU64::new(escape_timeout_ms(
        lock_core(&core).mux_options(),
    )));
    let mut terminal = TerminalGuard::enter(
        if mouse_option_enabled(lock_core(&core).mux_options()) {
            MouseArming::Button
        } else {
            MouseArming::Off
        },
        extended_keys,
        focus_events,
        clear_on_attach,
    )
    .map_err(|error| error.to_string())?;
    let pixel_mouse = terminal.pixel_mouse();
    let key_releases = terminal.kitty_keyboard();
    let output = terminal.writer();
    let mut renderer = Renderer::with_writer(std::rc::Rc::clone(&output));
    let mut browser = BrowserState::new(browser_provider);
    let mut kitty_probe = KittyProbe::new(
        configured_frame_transport_override(),
        terminal.kitty_probe_sent(),
    );
    renderer.set_frame_transport(kitty_probe.transport());
    browser.set_transport(kitty_probe.transport(), Instant::now());
    let kitty_gate = Arc::new(AtomicU8::new(KITTY_GATE_PROBING));
    let (events, incoming) = mpsc::channel();
    let mut frames = Arc::new(FrameInbox::default());
    let mut kitty_images = Arc::new(KittyImageInbox::default());
    let mut connection_id = 1;
    for message in initial_messages
        .into_iter()
        .chain(std::iter::from_fn(|| client.take_pending_message()))
    {
        if !forward_protocol_message(
            &core,
            message,
            connection_id,
            &events,
            &frames,
            &kitty_images,
            &kitty_gate,
            |outbound| match outbound {
                Outbound::RequestFull(pane) => client.request_full(pane),
                Outbound::TreeSync => client.request_tree_sync(),
            },
        ) {
            return Err("main event channel disconnected".to_owned());
        }
    }
    let mut model = Model::new(
        &lock_core(&core),
        size,
        host_label,
        local_host_label,
        endpoint.clone(),
        local_endpoint,
        fleet_hosts,
    );
    model.update_snapshot(Arc::clone(lock_core(&core).snapshot()));
    model.begin_client_focus_attach();
    let mut event_loop = EventLoop::new(&client).map_err(|error| error.to_string())?;
    event_loop.adopt_negotiation(&core);
    if terminal.kitty_probe_sent() {
        event_loop.await_graphics_reply();
    }
    event_loop.request_tty(matches!(endpoint, Endpoint::Local(_)));

    let mut attempt = attempt;
    let mut creating_default = false;
    let mut remembered_session = None;
    let mut reconnect_available = true;
    let mut deferred = None;

    let outcome = loop {
        event_loop.report_tty(|| {
            input::plain_key_pane(
                &model,
                &browser,
                lock_core(&core).prefix_armed(),
                key_releases,
            )
        });
        if browser.wants_graphics() {
            start_kitty_probe(&mut kitty_probe, &mut terminal, &mut event_loop)?;
        }
        let mut now = Instant::now();
        if model.expire_client_message(now) {
            renderer
                .paint(&model, false)
                .map_err(|error| error.to_string())?;
            now = Instant::now();
        }
        let event = if let Some(event) = deferred.take() {
            Some(event)
        } else if browser.should_pump(now) {
            None
        } else {
            event_loop.receive(
                now,
                click_wait(&model, message_wait(&model, browser.wait(now), now), now),
                &incoming,
                &events,
                &client,
                &core,
                connection_id,
                &frames,
                &kitty_images,
                &kitty_gate,
                &escape_time,
                &output,
            )?
        };
        let Some(event) = event else {
            let now = Instant::now();
            if model
                .click
                .as_ref()
                .is_some_and(|sequence| sequence.deadline <= now)
            {
                input::expire_click_sequence(&mut model, &client)?;
            }
            if pump_browser_provider(&mut browser, &mut renderer, &model, &client, Instant::now())?
            {
                renderer
                    .paint(&model, false)
                    .map_err(|error| error.to_string())?;
            }
            remembered_session = model.attached_session.or(remembered_session);
            continue;
        };
        match event {
            event @ (MainEvent::Frames(_) | MainEvent::Core { .. } | MainEvent::KittyImages(_)) => {
                let mut paint = PendingPaint::None;
                let mut core_seen = false;
                let mut exit = None;
                let mut handled = 0_usize;
                let mut next = Some(event);
                while let Some(event) = next.take() {
                    match event {
                        MainEvent::Frames(event_connection) => {
                            if event_connection == connection_id {
                                take_frames(&frames, &mut model, &mut renderer);
                                paint = paint.max(PendingPaint::Frames);
                            }
                        }
                        MainEvent::KittyImages(event_connection) => {
                            if event_connection == connection_id {
                                let updates = kitty_images.take();
                                let accept_images = kitty_probe.state != KittyProbeState::Disabled;
                                if accept_images
                                    && updates
                                        .iter()
                                        .any(|update| !matches!(update, KittyImageUpdate::Reset))
                                {
                                    start_kitty_probe(
                                        &mut kitty_probe,
                                        &mut terminal,
                                        &mut event_loop,
                                    )?;
                                }
                                let changed = !updates.is_empty();
                                apply_kitty_updates(&mut renderer, updates, accept_images);
                                if changed && kitty_probe.state == KittyProbeState::Enabled {
                                    paint = paint.max(PendingPaint::Repaint);
                                }
                            }
                        }
                        MainEvent::Core { connection, event } => {
                            if connection == connection_id {
                                core_seen = true;
                                if let CoreEvent::Attached { .. } = &*event {
                                    {
                                        let core = lock_core(&core);
                                        read_only = core.attached_read_only();
                                        client_flags = (!core.attached_client_flags().is_empty())
                                            .then(|| core.attached_client_flags().to_owned());
                                    }
                                    browser.reset_connection();
                                    reconnect_available = true;
                                }
                                if matches!(
                                    &*event,
                                    CoreEvent::MuxOptionsChanged
                                        | CoreEvent::HelloReceived
                                        | CoreEvent::Attached { .. }
                                        | CoreEvent::KeyTablesChanged
                                ) {
                                    refresh_terminal_options(&mut model, &core, &escape_time);
                                    event_loop.adopt_negotiation(&core);
                                    if crate::tty::take_utf8_repaint() {
                                        renderer
                                            .paint(&model, true)
                                            .map_err(|error| error.to_string())?;
                                    }
                                }
                                if let CoreEvent::PaneRemoved { pane } = &*event {
                                    frames.remove(*pane);
                                    renderer.forget_pane(*pane);
                                }
                                let outcome = handle_core_event(
                                    &mut model,
                                    &core,
                                    &client,
                                    *event,
                                    &mut attempt,
                                    &mut creating_default,
                                    &mut browser,
                                )?;
                                match outcome {
                                    ProtocolOutcome::None => {}
                                    ProtocolOutcome::Repaint => {
                                        paint = paint.max(PendingPaint::Repaint);
                                    }
                                    ProtocolOutcome::RepaintAll => {
                                        paint = paint.max(PendingPaint::RepaintAll);
                                    }
                                    ProtocolOutcome::QueueControl(output) => {
                                        renderer.queue_control(output);
                                        paint = paint.max(PendingPaint::Repaint);
                                    }
                                    ProtocolOutcome::Exit(reason) => {
                                        exit = Some(reason);
                                        break;
                                    }
                                }
                            }
                        }
                        other => {
                            deferred = Some(other);
                            break;
                        }
                    }
                    handled += 1;
                    if handled >= MAX_COALESCED_EVENTS {
                        break;
                    }
                    next = next_paint_event(&incoming, &mut deferred);
                }
                if let Some(reason) = exit {
                    break Ok(reason);
                }
                if let Some(sequence) = sync_mouse_modes(&mut model, pixel_mouse) {
                    renderer.queue_control(sequence);
                    paint = paint.max(if core_seen {
                        PendingPaint::Repaint
                    } else {
                        PendingPaint::Frames
                    });
                }
                paint_pending(paint, &mut model, &client, &mut browser, &mut renderer)?;
            }
            MainEvent::ClipboardQuery(event_connection) => {
                if event_connection == connection_id {
                    event_loop.await_clipboard_reply();
                    renderer.queue_control(clipboard::QUERY.to_vec());
                    renderer
                        .paint(&model, false)
                        .map_err(|error| error.to_string())?;
                }
            }
            MainEvent::Terminal(Ok(event)) => {
                let probe_update = kitty_probe.observe(&event);
                if probe_update.finish_file_probe {
                    terminal.finish_file_probe();
                }
                if let Some(transport) = probe_update.transport {
                    let now = Instant::now();
                    renderer.set_frame_transport(transport);
                    browser.set_transport(transport, now);
                    sync_browser_surfaces(&model, &mut browser, &mut renderer, now);
                    if kitty_probe.state == KittyProbeState::Enabled {
                        renderer
                            .paint(&model, false)
                            .map_err(|error| error.to_string())?;
                    }
                }
                if let Some(enabled) = probe_update.graphics {
                    if enabled {
                        kitty_gate.store(KITTY_GATE_ENABLED, Ordering::Release);
                        terminal.activate_kitty_graphics();
                        renderer.enable_kitty_graphics();
                        browser.enable();
                        sync_browser_surfaces(&model, &mut browser, &mut renderer, Instant::now());
                        renderer
                            .paint(&model, false)
                            .map_err(|error| error.to_string())?;
                    } else {
                        kitty_gate.store(KITTY_GATE_DISABLED, Ordering::Release);
                        kitty_images.clear();
                        browser.disable();
                        renderer.disable_kitty_graphics();
                    }
                }
                if probe_update.consumed {
                    continue;
                }
                let prefix = if let TerminalEvent::Key(event) = &event {
                    let input = input::key_chord(*event);
                    let core = lock_core(&core);
                    PrefixView {
                        armed: core.prefix_armed(),
                        claimed: core.claims_prefix_input(&input),
                    }
                } else {
                    PrefixView::default()
                };
                match input::handle(
                    &mut model,
                    &client,
                    &mut browser,
                    event,
                    pixel_mouse,
                    prefix,
                )? {
                    InputOutcome::None => {}
                    InputOutcome::Repaint => renderer
                        .paint(&model, false)
                        .map_err(|error| error.to_string())?,
                    InputOutcome::RepaintAll => {
                        send_resizes_and_sync_browser(
                            &mut model,
                            &client,
                            &mut browser,
                            &mut renderer,
                        )?;
                        renderer.invalidate();
                        renderer
                            .paint(&model, true)
                            .map_err(|error| error.to_string())?;
                    }
                    InputOutcome::Resize(size) => {
                        let unchanged = size == model.size;
                        let same_grid =
                            (size.columns, size.rows) == (model.size.columns, model.size.rows);
                        model.set_size(size);
                        if size.columns > 0 && size.rows > 0 && !same_grid {
                            client
                                .send_input(InputMessage::ClientTerminalSizeV2 {
                                    layout_generation: model.layout_generation,
                                    columns: size.columns,
                                    rows: size.rows,
                                })
                                .map_err(|error| error.to_string())?;
                        }
                        if unchanged {
                            continue;
                        }
                        send_resizes_and_sync_browser(
                            &mut model,
                            &client,
                            &mut browser,
                            &mut renderer,
                        )?;
                        renderer.invalidate();
                        renderer
                            .paint(&model, true)
                            .map_err(|error| error.to_string())?;
                    }
                    InputOutcome::AttachRequested => {
                        attempt = AttachAttempt::Explicit;
                        creating_default = false;
                        renderer
                            .paint(&model, false)
                            .map_err(|error| error.to_string())?;
                    }
                    InputOutcome::SwitchHost(host) => {
                        let label = host.label.clone();
                        match prepare_host_switch(&endpoint, host, |target| {
                            prepare_connection(target, String::new(), false, None)
                        }) {
                            Ok(HostSwitchDecision::Current) => {}
                            Err(error) => {
                                model.client_message = Some(ClientMessage::local(format!(
                                    "could not connect to {label}: {error}"
                                )));
                                renderer
                                    .paint(&model, false)
                                    .map_err(|paint| paint.to_string())?;
                            }
                            Ok(HostSwitchDecision::Switch { host, connected }) => {
                                let next_endpoint = host.endpoint.clone();
                                let replacement = replace_connection(
                                    &mut client,
                                    &mut core,
                                    &mut event_loop,
                                    &mut connection_id,
                                    connected,
                                    matches!(next_endpoint, Endpoint::Local(_)),
                                    &events,
                                    &mut frames,
                                    &mut kitty_images,
                                    &kitty_gate,
                                );
                                if let Err(error) = replacement {
                                    model.client_message = Some(ClientMessage::local(format!(
                                        "could not switch to {label}: {error}"
                                    )));
                                    renderer
                                        .paint(&model, false)
                                        .map_err(|paint| paint.to_string())?;
                                } else {
                                    browser.reset_connection();
                                    renderer.reset_kitty_images();
                                    read_only = false;
                                    client_flags = None;
                                    endpoint = next_endpoint;
                                    model.set_connected_host(host, &lock_core(&core));
                                    model.begin_client_focus_attach();
                                    refresh_terminal_options(&mut model, &core, &escape_time);
                                    if let Some(sequence) =
                                        sync_mouse_modes(&mut model, pixel_mouse)
                                    {
                                        renderer.queue_control(sequence);
                                    }
                                    model.client_message =
                                        Some(ClientMessage::local(format!("connected to {label}")));
                                    attempt = AttachAttempt::Default;
                                    creating_default = false;
                                    remembered_session = None;
                                    reconnect_available = true;
                                    renderer.invalidate();
                                    renderer
                                        .paint(&model, true)
                                        .map_err(|paint| paint.to_string())?;
                                }
                            }
                        }
                    }
                }
            }
            MainEvent::Terminal(Err(error)) => break Err(error),
            MainEvent::Disconnected { connection, error } => {
                if connection != connection_id {
                    continue;
                }
                if !reconnect_available {
                    break Ok(TuiExit::ServerExitedUnexpectedly);
                }
                reconnect_available = false;
                log::warn!("zz-tui connection closed: {error}");
                let session = remembered_session.or(model.attached_session);
                let Ok(replacement) = prepare_connection(
                    &endpoint,
                    session.map_or_else(String::new, |session| session.to_string()),
                    read_only,
                    client_flags.as_deref(),
                ) else {
                    break Ok(TuiExit::ServerExitedUnexpectedly);
                };
                attempt = if session.is_some() {
                    AttachAttempt::Remembered
                } else {
                    AttachAttempt::Default
                };
                creating_default = false;
                if replace_connection(
                    &mut client,
                    &mut core,
                    &mut event_loop,
                    &mut connection_id,
                    replacement,
                    matches!(endpoint, Endpoint::Local(_)),
                    &events,
                    &mut frames,
                    &mut kitty_images,
                    &kitty_gate,
                )
                .is_err()
                {
                    break Ok(TuiExit::ServerExitedUnexpectedly);
                }
                browser.reset_connection();
                renderer.reset_kitty_images();
                model.reset_connection(&lock_core(&core));
                model.begin_client_focus_attach();
                refresh_terminal_options(&mut model, &core, &escape_time);
                if let Some(sequence) = sync_mouse_modes(&mut model, pixel_mouse) {
                    renderer.queue_control(sequence);
                }
                model.client_message = Some(ClientMessage::local("reconnected"));
                renderer.invalidate();
                renderer
                    .paint(&model, true)
                    .map_err(|paint| paint.to_string())?;
            }
            MainEvent::Resize => {
                if let Ok(size) = TerminalSize::detect() {
                    let previous = model.size;
                    model.set_size(size);
                    crate::overlay::close_display_panes_on_resize(&model, &client, previous)?;
                    if size.columns > 0 && size.rows > 0 {
                        client
                            .send_input(InputMessage::ClientTerminalSizeV2 {
                                layout_generation: model.layout_generation,
                                columns: size.columns,
                                rows: size.rows,
                            })
                            .map_err(|error| error.to_string())?;
                    }
                    if size != previous {
                        send_resizes_and_sync_browser(
                            &mut model,
                            &client,
                            &mut browser,
                            &mut renderer,
                        )?;
                    }
                    renderer.invalidate();
                    renderer
                        .paint(&model, true)
                        .map_err(|error| error.to_string())?;
                }
            }
            MainEvent::Suspend => {
                client
                    .send_input(InputMessage::ClientSuspendState { suspended: true })
                    .map_err(|error| error.to_string())?;
                renderer.pause(true);
                event_loop.release_tty();
                terminal.suspend();
                #[cfg(unix)]
                rustix::process::kill_process(
                    rustix::process::getpid(),
                    rustix::process::Signal::STOP,
                )
                .map_err(|error| error.to_string())?;
            }
            MainEvent::Resume => {
                client
                    .send_input(InputMessage::ClientSuspendState { suspended: false })
                    .map_err(|error| error.to_string())?;
                #[cfg(unix)]
                terminal
                    .resume(
                        model.mouse_arming,
                        extended_keys,
                        focus_events,
                        crate::tty::clear_on_attach_enabled(lock_core(&core).mux_options()),
                    )
                    .map_err(|error| error.to_string())?;
                if terminal.kitty_probe_sent() {
                    event_loop.await_graphics_reply();
                }
                event_loop.request_tty(matches!(endpoint, Endpoint::Local(_)));
                renderer.pause(false);
                renderer
                    .paint(&model, true)
                    .map_err(|error| error.to_string())?;
                let _ = events.send(MainEvent::Resize);
            }
            MainEvent::Signal => break Ok(TuiExit::Detached(attached_session_name(&model))),
            MainEvent::Repaint => {
                renderer.invalidate();
                renderer
                    .paint(&model, true)
                    .map_err(|error| error.to_string())?;
            }
        }
        remembered_session = model.attached_session.or(remembered_session);
    };

    event_loop.release_tty();
    browser.close_all();
    renderer.discard_queued_paints();
    drop(terminal);
    let deadline = Instant::now() + Duration::from_secs(1);
    while output.borrow().pending_fd().is_some() && Instant::now() < deadline {
        output
            .borrow_mut()
            .flush()
            .map_err(|error| error.to_string())?;
        std::thread::yield_now();
    }
    match outcome {
        Ok(TuiExit::Exec { command, shell }) => Err(exec_client_command(&shell, &command)),
        Ok(exit) => {
            println!("{}", exit.notice());
            let hangup = matches!(exit, TuiExit::DetachedHangup(_));
            if exit.exit_code() == 0 {
                if hangup {
                    hangup_parent();
                }
                Ok(())
            } else {
                std::process::exit(i32::from(exit.exit_code()))
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
pub(crate) fn run(
    _initial: InteractiveClient,
    _endpoint: Endpoint,
    _local_endpoint: Endpoint,
    _initial_attach: InitialAttach,
    _terminal_options: Option<TerminalOptions>,
    _host_label: String,
    _local_host_label: String,
    _fleet_hosts: Vec<HostEntry>,
    _browser_provider: Option<Box<dyn BrowserFrameProvider>>,
) -> Result<(), String> {
    Err("zz-tui currently requires a Unix terminal".to_owned())
}

#[cfg(unix)]
fn hangup_parent() {
    use rustix::process::{Signal, getppid, kill_process};

    let Some(parent) = getppid() else {
        return;
    };
    if parent.as_raw_nonzero().get() > 1 {
        let _ = kill_process(parent, Signal::HUP);
    }
}

#[cfg(not(unix))]
const fn hangup_parent() {}

#[cfg(unix)]
fn start_kitty_probe(
    probe: &mut KittyProbe,
    terminal: &mut TerminalGuard,
    event_loop: &mut EventLoop,
) -> Result<(), String> {
    if probe.start() {
        event_loop.await_graphics_reply();
        terminal
            .probe_kitty_graphics()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn take_frames(frames: &FrameInbox, model: &mut Model, renderer: &mut Renderer) {
    let mut pending = frames.take();
    for (pane, frame) in pending.drain() {
        if !model.accepts_viewport(pane) {
            model.viewports.remove(&pane);
            renderer.forget_pane(pane);
            continue;
        }
        model.viewports.insert(pane, frame.viewport);
        renderer.note_frame(pane, frame.damage);
    }
    frames.recycle(pending);
}

fn paint_pending(
    paint: PendingPaint,
    model: &mut Model,
    client: &InteractiveClient,
    browser: &mut BrowserState,
    renderer: &mut Renderer,
) -> Result<(), String> {
    match paint {
        PendingPaint::None => Ok(()),
        PendingPaint::Frames => renderer
            .paint_frames(model)
            .map_err(|error| error.to_string()),
        PendingPaint::Repaint => renderer
            .paint(model, false)
            .map_err(|error| error.to_string()),
        PendingPaint::RepaintAll => {
            send_resizes_and_sync_browser(model, client, browser, renderer)?;
            renderer.invalidate();
            renderer
                .paint(model, true)
                .map_err(|error| error.to_string())
        }
    }
}

#[cfg(unix)]
fn exec_client_command(shell: &str, command: &str) -> String {
    use std::os::unix::process::CommandExt as _;

    let argv0 = std::path::Path::new(shell).file_name().map_or_else(
        || shell.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let error = std::process::Command::new(shell)
        .arg0(argv0)
        .arg("-c")
        .arg(command)
        .env("SHELL", shell)
        .exec();
    format!("{shell}: {error}")
}

#[cfg(not(unix))]
fn exec_client_command(shell: &str, _command: &str) -> String {
    format!("{shell}: detach-client -E needs a Unix client")
}

fn prepare_connection(
    endpoint: &Endpoint,
    attach_target: String,
    read_only: bool,
    client_flags: Option<&str>,
) -> Result<PreparedConnection, String> {
    let mut args = Vec::new();
    if read_only {
        args.push("-r".to_owned());
    }
    if let Some(flags) = client_flags {
        args.extend(["-f".to_owned(), flags.to_owned()]);
    }
    if !attach_target.is_empty() {
        args.extend(["-t".to_owned(), attach_target]);
    }
    let client = InteractiveClient::connect_endpoint_with_attach(
        endpoint,
        zz_protocol::AttachOperation::Commands(vec![zz_protocol::PreparedCommand {
            invocation: CommandInvocation::new("attach-session", args),
            canonical_name: Some("attach-session".to_owned()),
            alias_matched: false,
            result: zz_protocol::PreparedCommandResult::Ready,
        }]),
    )
    .map_err(|error| error.to_string())?;
    let core = seeded_core(client.server_hello().clone());
    let client = Arc::new(client);
    Ok(PreparedConnection { client, core })
}

fn seeded_core(hello: ServerHello) -> Arc<Mutex<ClientCore>> {
    let mut core = ClientCore::new();
    core.handle_message(ProtocolMessage::ServerHello(Box::new(hello)));
    while core.poll_event().is_some() {}
    Arc::new(Mutex::new(core))
}

fn lock_core(core: &Mutex<ClientCore>) -> MutexGuard<'_, ClientCore> {
    core.lock().expect("client core poisoned")
}

pub(crate) fn mouse_option_enabled(options: &zz_protocol::MuxOptions) -> bool {
    options
        .get(zz_protocol::MuxOptionKey::Mouse)
        .is_some_and(|option| option.value == "on")
}

pub(crate) fn focus_follows_mouse_enabled(options: &zz_protocol::MuxOptions) -> bool {
    options
        .get(zz_protocol::MuxOptionKey::FocusFollowsMouse)
        .is_some_and(|option| option.value == "on")
}

fn escape_timeout_ms(options: &zz_protocol::MuxOptions) -> u64 {
    options
        .get(zz_protocol::MuxOptionKey::EscapeTime)
        .and_then(|option| option.value.parse::<u64>().ok())
        .unwrap_or(10)
        .max(1)
}

fn refresh_terminal_options(model: &mut Model, core: &Mutex<ClientCore>, escape_time: &AtomicU64) {
    let core = lock_core(core);
    let options = core.mux_options();
    escape_time.store(escape_timeout_ms(options), Ordering::Relaxed);
    model.mouse_option = mouse_option_enabled(options);
    model.focus_follows_mouse = focus_follows_mouse_enabled(options);
    model.mouse_bindings = core.mouse_bindings().keys().into_iter().collect();
    model.copy_mouse_bindings = core.mouse_bindings().copy_keys().into_iter().collect();
}

#[cfg(test)]
pub(crate) fn mouse_binding_names(
    tables: &[zz_protocol::KeyTableSnapshot],
) -> std::collections::HashSet<String> {
    zz_protocol::MouseBindings::from_tables(tables)
        .keys()
        .into_iter()
        .collect()
}

fn desired_mouse_arming(model: &Model) -> MouseArming {
    let overlay_any = model.menu.as_ref().map(|menu| menu.mouse_keys);
    if !model.mouse_option {
        let tracking = overlay_any.unwrap_or_else(|| {
            model
                .active_viewport()
                .is_some_and(|viewport| viewport.mouse_tracking)
        });
        return if tracking {
            MouseArming::Any
        } else {
            MouseArming::Off
        };
    }
    let any = overlay_any.unwrap_or_else(|| {
        model.layout.panes.iter().any(|entry| {
            model
                .viewports
                .get(&entry.pane)
                .is_some_and(|viewport| viewport.mouse_tracking)
        })
    });
    if any || model.focus_follows_mouse {
        MouseArming::Any
    } else {
        MouseArming::Button
    }
}

fn sync_mouse_modes(model: &mut Model, pixel_mouse: bool) -> Option<Vec<u8>> {
    let desired = desired_mouse_arming(model);
    if desired == model.mouse_arming {
        return None;
    }
    model.mouse_arming = desired;
    Some(crate::tty::mouse_mode_sequence(desired, pixel_mouse))
}

fn prepare_host_switch<T>(
    current_endpoint: &Endpoint,
    host: HostSwitch,
    connect: impl FnOnce(&Endpoint) -> Result<T, String>,
) -> Result<HostSwitchDecision<T>, String> {
    if &host.endpoint == current_endpoint {
        return Ok(HostSwitchDecision::Current);
    }
    let connected = connect(&host.endpoint)?;
    Ok(HostSwitchDecision::Switch { host, connected })
}

#[cfg(unix)]
fn replace_connection(
    client: &mut Arc<InteractiveClient>,
    core: &mut Arc<Mutex<ClientCore>>,
    event_loop: &mut EventLoop,
    connection_id: &mut u64,
    connected: PreparedConnection,
    local: bool,
    _events: &mpsc::Sender<MainEvent>,
    frames: &mut Arc<FrameInbox>,
    kitty_images: &mut Arc<KittyImageInbox>,
    _kitty_gate: &Arc<AtomicU8>,
) -> Result<(), String> {
    event_loop
        .replace(&connected.client, local)
        .map_err(|error| error.to_string())?;
    kitty_images.clear();
    *client = connected.client;
    *core = connected.core;
    *connection_id = connection_id.wrapping_add(1).max(1);
    *frames = Arc::new(FrameInbox::default());
    *kitty_images = Arc::new(KittyImageInbox::default());
    Ok(())
}

fn forward_protocol_message(
    core: &Mutex<ClientCore>,
    message: ProtocolMessage,
    connection: u64,
    events: &mpsc::Sender<MainEvent>,
    frames: &FrameInbox,
    kitty_images: &KittyImageInbox,
    kitty_gate: &AtomicU8,
    mut send_outbound: impl FnMut(Outbound) -> Result<(), zz_daemon_client::DaemonError>,
) -> bool {
    let mut core = lock_core(core);
    core.handle_message(message);
    if core.take_clipboard_query() && events.send(MainEvent::ClipboardQuery(connection)).is_err() {
        return false;
    }
    while let Some(outbound) = core.poll_outbound() {
        if let Err(error) = send_outbound(outbound) {
            log::warn!("failed to synchronize the terminal client: {error}");
        }
    }
    while let Some(event) = core.poll_event() {
        let event = match event {
            CoreEvent::ViewportChanged { pane, damage } => {
                if let Some(viewport) = core.viewport(pane) {
                    frames.publish(pane, viewport.clone(), damage, connection, events);
                }
                continue;
            }
            CoreEvent::KittyImageBegin {
                pane,
                image_id,
                generation,
                width,
                height,
                total_bytes,
            } => {
                if kitty_gate.load(Ordering::Acquire) != KITTY_GATE_DISABLED {
                    kitty_images.begin(pane, image_id, generation, width, height, total_bytes);
                }
                continue;
            }
            CoreEvent::KittyImageChunk {
                pane,
                image_id,
                generation,
                bytes,
            } => {
                if kitty_gate.load(Ordering::Acquire) != KITTY_GATE_DISABLED {
                    kitty_images.push_chunk(pane, image_id, generation, bytes, connection, events);
                }
                continue;
            }
            CoreEvent::KittyImagesRemoved { pane, image_ids } => {
                if kitty_gate.load(Ordering::Acquire) != KITTY_GATE_DISABLED {
                    kitty_images.remove(pane, image_ids, connection, events);
                }
                continue;
            }
            CoreEvent::Attached { session } => {
                frames.clear();
                if events
                    .send(MainEvent::Core {
                        connection,
                        event: Box::new(CoreEvent::Attached { session }),
                    })
                    .is_err()
                {
                    return false;
                }
                kitty_images.reset_attachment(connection, events);
                continue;
            }
            CoreEvent::PaneRemoved { pane } => {
                kitty_images.remove_pane(pane);
                CoreEvent::PaneRemoved { pane }
            }
            event => event,
        };
        if events
            .send(MainEvent::Core {
                connection,
                event: Box::new(event),
            })
            .is_err()
        {
            return false;
        }
    }
    true
}

fn handle_core_event(
    model: &mut Model,
    core: &Mutex<ClientCore>,
    client: &InteractiveClient,
    event: CoreEvent,
    attempt: &mut AttachAttempt,
    creating_default: &mut bool,
    browser: &mut BrowserState,
) -> Result<ProtocolOutcome, String> {
    match event {
        CoreEvent::Attached { session } => {
            *attempt = AttachAttempt::Idle;
            model.attached_session = Some(session);
            model.set_command_output(None, None);
            model.set_menu(None);
            model.confirm = None;
            model.confirm_reply_pending = false;
            model.client_message = None;
            let (snapshot, viewports, layout_generation) = {
                let core = lock_core(core);
                let snapshot = Arc::clone(core.snapshot());
                let viewports = snapshot
                    .sessions
                    .iter()
                    .flat_map(|session| &session.windows)
                    .flat_map(|window| window.panes.keys())
                    .filter_map(|pane| Some((*pane, core.viewport(*pane)?.clone())))
                    .collect();
                (snapshot, viewports, core.layout_generation())
            };
            update_snapshot(model, snapshot, layout_generation);
            model.viewports = viewports;
            if let Some(input) = model.finish_client_focus_attach() {
                client
                    .send_input(input)
                    .map_err(|error| error.to_string())?;
            }
            *creating_default = false;
            Ok(ProtocolOutcome::RepaintAll)
        }
        CoreEvent::SnapshotChanged => {
            let (snapshot, layout_generation) = {
                let core = lock_core(core);
                (Arc::clone(core.snapshot()), core.layout_generation())
            };
            refresh_snapshot(model, snapshot, layout_generation, |input| {
                client.send_input(input).map_err(|error| error.to_string())
            })
        }
        CoreEvent::AppearanceChanged => {
            model.appearance = lock_core(core).appearance().cloned().unwrap_or_default();
            Ok(ProtocolOutcome::RepaintAll)
        }
        CoreEvent::StatusChanged => {
            let status = lock_core(core).status().clone();
            if model.set_status(status) {
                Ok(ProtocolOutcome::RepaintAll)
            } else {
                Ok(ProtocolOutcome::Repaint)
            }
        }
        CoreEvent::PrefixArmed { armed } => {
            model.prefix_armed = armed;
            Ok(ProtocolOutcome::Repaint)
        }
        CoreEvent::CommandPromptChanged => {
            let prompt = lock_core(core).command_prompt().cloned();
            let closed = model.command_prompt.is_some() && prompt.is_none();
            model.command_prompt = prompt;
            Ok(if closed {
                ProtocolOutcome::RepaintAll
            } else {
                ProtocolOutcome::Repaint
            })
        }
        CoreEvent::CommandOutputChanged => {
            let (output_id, output) = {
                let core = lock_core(core);
                let output = core
                    .command_output()
                    .map(|(pane, viewport)| (pane, viewport.clone()));
                (core.command_output_id(), output)
            };
            model.set_command_output(output_id, output);
            Ok(ProtocolOutcome::RepaintAll)
        }
        CoreEvent::ChooseTreeChanged => {
            let core = lock_core(core);
            model.choose_tree = core.choose_tree().cloned();
            model.chooser_presentation = core.chooser_presentation().cloned();
            Ok(ProtocolOutcome::RepaintAll)
        }
        CoreEvent::ChooseBufferChanged => {
            let core = lock_core(core);
            model.choose_buffer = core.choose_buffer().cloned();
            model.chooser_presentation = core.chooser_presentation().cloned();
            Ok(ProtocolOutcome::RepaintAll)
        }
        CoreEvent::DisplayPanesChanged => {
            model.display_panes = lock_core(core).display_panes().cloned();
            Ok(ProtocolOutcome::RepaintAll)
        }
        CoreEvent::MenuChanged => {
            model.sync_menu(&lock_core(core));
            Ok(ProtocolOutcome::RepaintAll)
        }
        CoreEvent::ConfirmChanged => {
            model.confirm = lock_core(core).confirm().cloned();
            model.confirm_reply_pending = false;
            Ok(ProtocolOutcome::RepaintAll)
        }
        CoreEvent::ClientMessage {
            text,
            duration_ms,
            message_id,
            ..
        } => {
            model.client_message = Some(ClientMessage::timed(
                text,
                message_id,
                duration_ms,
                Instant::now(),
            ));
            Ok(ProtocolOutcome::Repaint)
        }
        CoreEvent::ClientMessageCleared { message_id } => {
            if model.clear_client_message(message_id) {
                Ok(ProtocolOutcome::Repaint)
            } else {
                Ok(ProtocolOutcome::None)
            }
        }
        CoreEvent::PaneRemoved { pane } => {
            model.input_event(InputEvent::PaneRemoved(pane));
            model.viewports.remove(&pane);
            Ok(ProtocolOutcome::RepaintAll)
        }
        CoreEvent::Detached {
            session,
            by: _,
            action,
        } if model.attached_session == Some(session) => {
            model.input_event(InputEvent::Detached);
            let core = lock_core(core);
            let exit = if let ClientExitAction::Exec { command, shell } = action {
                TuiExit::Exec { command, shell }
            } else if core.last_detach_was_session_destroyed() {
                TuiExit::Exited
            } else if core.last_detach_was_server_stopping() {
                TuiExit::ServerExited
            } else if action.is_parent_hangup() {
                TuiExit::DetachedHangup(attached_session_name(model))
            } else {
                TuiExit::Detached(attached_session_name(model))
            };
            Ok(ProtocolOutcome::Exit(exit))
        }
        CoreEvent::ServerStopping => Ok(ProtocolOutcome::Exit(TuiExit::ServerExited)),
        CoreEvent::Clipboard {
            producer,
            target,
            text,
            ..
        } => match clipboard::encode(clipboard::Selection::for_producer(producer, target), &text) {
            Osc52::Empty => Ok(ProtocolOutcome::None),
            Osc52::Encoded(output) => Ok(ProtocolOutcome::QueueControl(output)),
            Osc52::TooLarge => {
                model.client_message =
                    Some(ClientMessage::local("clipboard payload exceeds 1 MiB"));
                Ok(ProtocolOutcome::Repaint)
            }
        },
        CoreEvent::FocusSidebar => {
            if model.focus_sidebar() {
                Ok(ProtocolOutcome::RepaintAll)
            } else {
                Ok(ProtocolOutcome::Repaint)
            }
        }
        CoreEvent::BrowserCommand {
            command: BrowserCommand::Screenshot { request_id, .. },
            ..
        } => {
            client
                .send_gui_response(GuiResponse::Error {
                    request_id,
                    message: "browser screenshots require the zz app".to_owned(),
                })
                .map_err(|error| error.to_string())?;
            Ok(ProtocolOutcome::None)
        }
        CoreEvent::BrowserCommand { pane, command } => {
            browser.command(pane, &command);
            Ok(ProtocolOutcome::None)
        }
        CoreEvent::TerminalUiCommand { pane, command } => {
            if let Some(action) = command_output_ui_action(model, pane, command) {
                client
                    .send_input(InputMessage::CommandOutputView { action })
                    .map_err(|error| error.to_string())?;
                Ok(ProtocolOutcome::Repaint)
            } else {
                model.client_message =
                    Some(ClientMessage::local("terminal search is unsupported here"));
                Ok(ProtocolOutcome::Repaint)
            }
        }
        CoreEvent::CommandResponse(response) => {
            handle_command_response(model, core, client, response, attempt, creating_default)
        }
        CoreEvent::HelloReceived
        | CoreEvent::Detached { .. }
        | CoreEvent::ViewportChanged { .. }
        | CoreEvent::MuxOptionsChanged
        | CoreEvent::KeyTablesChanged
        | CoreEvent::KeyTableChanged
        | CoreEvent::PrefixCancelled { .. }
        | CoreEvent::Bell { .. }
        | CoreEvent::OpenPathPicker { .. }
        | CoreEvent::OpenUri { .. }
        | CoreEvent::HistoryChunk { .. }
        | CoreEvent::KittyImageBegin { .. }
        | CoreEvent::KittyImageChunk { .. }
        | CoreEvent::KittyImagesRemoved { .. }
        | CoreEvent::AgentUpdates { .. }
        | CoreEvent::AgentStateChanged { .. }
        | CoreEvent::AgentLagged { .. }
        | CoreEvent::AgentReplayed { .. }
        | CoreEvent::AgentSessions { .. }
        | CoreEvent::ProvidersChanged
        | CoreEvent::AgentAccountsChanged
        | CoreEvent::AgentLoginChanged { .. }
        | CoreEvent::SimulatorChanged { .. }
        | CoreEvent::SimulatorCommand { .. }
        | CoreEvent::Message(_) => Ok(ProtocolOutcome::None),
    }
}

fn command_output_ui_action(
    model: &mut Model,
    pane: PaneId,
    command: TerminalUiCommand,
) -> Option<TerminalViewAction> {
    let active = model
        .command_output
        .as_ref()
        .is_some_and(|(output_pane, _)| *output_pane == pane);
    if !active {
        return None;
    }
    match command {
        TerminalUiCommand::BeginSearch { direction } => {
            let query = SearchQuery {
                direction,
                ..SearchQuery::default()
            };
            model.command_output_search = Some(query.clone());
            model.command_output_swallowed_key = None;
            Some(TerminalViewAction::SearchBegin(query))
        }
    }
}

fn attached_session_name(model: &Model) -> String {
    model
        .attached_session
        .and_then(|attached| {
            model
                .snapshot
                .sessions
                .iter()
                .find(|session| session.id == attached)
        })
        .map(|session| session.name.clone())
        .or_else(|| model.attached_session.map(|session| session.to_string()))
        .unwrap_or_default()
}

fn handle_command_response(
    model: &mut Model,
    core: &Mutex<ClientCore>,
    client: &InteractiveClient,
    response: CommandResponse,
    attempt: &mut AttachAttempt,
    creating_default: &mut bool,
) -> Result<ProtocolOutcome, String> {
    match response {
        CommandResponse::Error {
            request_id: 0,
            error,
            ..
        } if attach_attempt_owns_missing_response(*attempt, &error) => {
            let (ServerError::MissingTarget(target) | ServerError::SessionNotFound(target)) = error
            else {
                unreachable!("attach response guard accepted a different error")
            };
            match *attempt {
                AttachAttempt::Remembered => {
                    if let Err(error) = client.attach("") {
                        recover_client_focus_after_attach_error(model, client)?;
                        *attempt = AttachAttempt::Idle;
                        return Err(error.to_string());
                    }
                    model.begin_client_focus_attach();
                    *attempt = AttachAttempt::Default;
                    Ok(ProtocolOutcome::None)
                }
                AttachAttempt::Default if !*creating_default => {
                    *creating_default = true;
                    let creation = (|| {
                        client.request_resync().map_err(|error| error.to_string())?;
                        client
                            .execute(CommandInvocation::new("new-session", [] as [&str; 0]))
                            .map_err(|error| error.to_string())?;
                        let attaches = lock_core(core)
                            .capabilities()
                            .iter()
                            .any(|capability| capability == NEW_SESSION_ATTACH_CAPABILITY);
                        if !attaches {
                            client
                                .execute(CommandInvocation::new("attach-session", [] as [&str; 0]))
                                .map_err(|error| error.to_string())?;
                        }
                        Ok(())
                    })();
                    if let Err(error) = creation {
                        recover_client_focus_after_attach_error(model, client)?;
                        *attempt = AttachAttempt::Idle;
                        return Err(error);
                    }
                    Ok(ProtocolOutcome::Repaint)
                }
                AttachAttempt::Default => Ok(ProtocolOutcome::None),
                AttachAttempt::Explicit if model.attached_session.is_some() => {
                    recover_client_focus_after_attach_error(model, client)?;
                    *attempt = AttachAttempt::Idle;
                    model.client_message = Some(ClientMessage::local(format!(
                        "session `{target}` was not found"
                    )));
                    Ok(ProtocolOutcome::Repaint)
                }
                AttachAttempt::Explicit => {
                    recover_client_focus_after_attach_error(model, client)?;
                    *attempt = AttachAttempt::Idle;
                    Err(format!("session `{target}` was not found"))
                }
                AttachAttempt::Idle => {
                    unreachable!("attach response guard requires a pending attempt")
                }
            }
        }
        CommandResponse::Error {
            request_id: 0,
            error: ServerError::PaneExited(_) | ServerError::PaneNotAttached(_),
            ..
        } => Ok(ProtocolOutcome::None),
        CommandResponse::Error {
            request_id: 0,
            error: ServerError::InvalidCommand(message),
            ..
        } if attempt.is_pending()
            && model.attached_session.is_none()
            && message == "sessions should be nested with care, unset $TMUX to force" =>
        {
            recover_client_focus_after_attach_error(model, client)?;
            *attempt = AttachAttempt::Idle;
            Err(message)
        }
        CommandResponse::Error { error, .. } => {
            model.client_message = Some(ClientMessage::local(error.tmux_message()));
            Ok(ProtocolOutcome::Repaint)
        }
        CommandResponse::Success { .. } => {
            model.client_message = None;
            Ok(ProtocolOutcome::Repaint)
        }
    }
}

fn recover_client_focus_after_attach_error(
    model: &mut Model,
    client: &InteractiveClient,
) -> Result<(), String> {
    if let Some(input) = model.fail_client_focus_attach() {
        client
            .send_input(input)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn message_wait(model: &Model, wait: BrowserWait, now: Instant) -> BrowserWait {
    let Some(deadline) = model.client_message_deadline() else {
        return wait;
    };
    let remaining = deadline.saturating_duration_since(now);
    match wait {
        BrowserWait::Blocking => BrowserWait::Timeout(remaining),
        BrowserWait::Timeout(timeout) => BrowserWait::Timeout(timeout.min(remaining)),
    }
}

fn click_wait(model: &Model, wait: BrowserWait, now: Instant) -> BrowserWait {
    let Some(sequence) = model.click.as_ref() else {
        return wait;
    };
    let remaining = sequence.deadline.saturating_duration_since(now);
    match wait {
        BrowserWait::Blocking => BrowserWait::Timeout(remaining),
        BrowserWait::Timeout(timeout) => BrowserWait::Timeout(timeout.min(remaining)),
    }
}

fn next_paint_event(
    incoming: &mpsc::Receiver<MainEvent>,
    deferred: &mut Option<MainEvent>,
) -> Option<MainEvent> {
    match incoming.try_recv().ok() {
        Some(
            event @ (MainEvent::Frames(_) | MainEvent::Core { .. } | MainEvent::KittyImages(_)),
        ) => Some(event),
        event => {
            *deferred = event;
            None
        }
    }
}

fn pump_browser_provider(
    browser: &mut BrowserState,
    renderer: &mut Renderer,
    model: &Model,
    client: &InteractiveClient,
    now: Instant,
) -> Result<bool, String> {
    let output = browser.pump(now);
    let changed = !output.frames.is_empty();
    for frame in output.frames {
        let pane = frame.image.pane;
        let transmitted = renderer.install_browser_frame(frame);
        browser.note_transmit_cost(pane, transmitted);
    }
    for (pane, tabs, active) in output.navigations {
        let Some(snapshot) = model.pane_snapshot(pane) else {
            continue;
        };
        let PaneKindSnapshot::Browser(current) = &snapshot.kind else {
            continue;
        };
        let Some(command) = set_browser_tabs_command(pane, current, tabs, active) else {
            continue;
        };
        client
            .execute(command)
            .map(drop)
            .map_err(|error| error.to_string())?;
    }
    Ok(changed)
}

fn set_browser_tabs_command(
    pane: PaneId,
    current: &BrowserDescriptor,
    tabs: Vec<String>,
    active: usize,
) -> Option<CommandInvocation> {
    if tabs.is_empty()
        || active >= tabs.len()
        || (current.tabs == tabs && current.active_tab == active)
    {
        return None;
    }
    let mut args = vec![
        "-t".to_owned(),
        pane.to_string(),
        "-a".to_owned(),
        active.to_string(),
        "--".to_owned(),
    ];
    args.extend(tabs);
    Some(CommandInvocation::new("set-browser-tabs", args))
}

const fn hidpi_scale(cell_height_px: u32) -> f32 {
    if cell_height_px >= 28 { 2.0 } else { 1.0 }
}

fn visible_browser_surfaces(model: &Model, max_surface_bytes: u64) -> Vec<BrowserSurface> {
    model
        .layout
        .panes
        .iter()
        .filter_map(|entry| {
            let snapshot = model.pane_snapshot(entry.pane)?;
            let PaneKindSnapshot::Browser(descriptor) = &snapshot.kind else {
                return None;
            };
            let content = entry.content();
            Some(BrowserSurface {
                pane: entry.pane,
                descriptor: descriptor.clone(),
                cells: (content.width, content.height),
                px: crate::browser::clamp_surface_px(
                    (
                        u32::from(content.width).saturating_mul(model.size.cell_width_px),
                        u32::from(content.height).saturating_mul(model.size.cell_height_px),
                    ),
                    max_surface_bytes,
                ),
                base_scale: hidpi_scale(model.size.cell_height_px),
            })
        })
        .collect()
}

fn sync_browser_surfaces(
    model: &Model,
    browser: &mut BrowserState,
    renderer: &mut Renderer,
    now: Instant,
) {
    let surfaces = visible_browser_surfaces(model, browser.surface_byte_budget());
    let SurfaceChanges { closed, resized } = browser.reconcile_surfaces(surfaces, now);
    for pane in closed {
        renderer.remove_browser_frame(pane);
    }
    for (pane, cells) in resized {
        renderer.resize_browser_frame(pane, cells);
    }
}

fn send_resizes_and_sync_browser(
    model: &mut Model,
    client: &InteractiveClient,
    browser: &mut BrowserState,
    renderer: &mut Renderer,
) -> Result<(), String> {
    send_resizes(model, client)?;
    sync_browser_surfaces(model, browser, renderer, Instant::now());
    Ok(())
}

fn send_resizes(model: &mut Model, client: &InteractiveClient) -> Result<(), String> {
    send_resizes_with(model, |input| {
        client.send_input(input).map_err(|error| error.to_string())
    })
}

fn refresh_snapshot(
    model: &mut Model,
    snapshot: Arc<zz_protocol::MuxSnapshot>,
    layout_generation: u64,
    send: impl FnMut(InputMessage) -> Result<(), String>,
) -> Result<ProtocolOutcome, String> {
    let before = model.paint_structure();
    if update_snapshot(model, snapshot, layout_generation) {
        send_resizes_with(model, send)?;
    }
    Ok(if model.paint_structure() == before {
        ProtocolOutcome::Repaint
    } else {
        ProtocolOutcome::RepaintAll
    })
}

fn update_snapshot(
    model: &mut Model,
    snapshot: Arc<zz_protocol::MuxSnapshot>,
    layout_generation: u64,
) -> bool {
    let generation_changed = model.layout_generation != layout_generation;
    if generation_changed {
        model.last_sent_geometry.clear();
    }
    model.layout_generation = layout_generation;
    model.update_snapshot(snapshot);
    generation_changed
}

fn send_resizes_with(
    model: &mut Model,
    mut send: impl FnMut(InputMessage) -> Result<(), String>,
) -> Result<(), String> {
    let geometries = model.terminal_geometries();
    let visible = geometries
        .iter()
        .map(|(pane, _)| *pane)
        .collect::<HashSet<_>>();
    model
        .last_sent_geometry
        .retain(|pane, _| visible.contains(pane));
    for (pane, geometry @ (columns, rows, cell_width_px, cell_height_px)) in geometries {
        if model.last_sent_geometry.get(&pane) == Some(&geometry) {
            continue;
        }
        send(InputMessage::ResizeTerminalV2 {
            pane,
            columns,
            rows,
            cell_width_px,
            cell_height_px,
            layout_generation: model.layout_generation,
        })?;
        model.last_sent_geometry.insert(pane, geometry);
    }
    if let Some((geometry, message)) = command_output_resize_message(model) {
        send(message)?;
        model.last_sent_command_output_geometry = Some(geometry);
    } else if model.command_output_geometry().is_none() {
        model.last_sent_command_output_geometry = None;
    }
    Ok(())
}

fn command_output_resize_message(model: &Model) -> Option<((u16, u16, u32, u32), InputMessage)> {
    let geometry @ (columns, rows, cell_width_px, cell_height_px) =
        model.command_output_geometry()?;
    (model.last_sent_command_output_geometry != Some(geometry)).then_some((
        geometry,
        InputMessage::ResizeCommandOutput {
            columns,
            rows,
            cell_width_px,
            cell_height_px,
        },
    ))
}

#[cfg(test)]
mod attach_paint_tests;

#[cfg(test)]
mod command_output_tests;

#[cfg(test)]
mod frame_inbox_tests;

#[cfg(test)]
mod graphics_probe_tests;

#[cfg(test)]
mod lifecycle_tests;

#[cfg(test)]
mod mouse_tests;

#[cfg(test)]
mod paint_tests;

#[cfg(test)]
mod test_support;
