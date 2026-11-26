#[cfg(any(not(unix), test))]
use std::io::Read;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    convert::Infallible,
    io::Write,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, LazyLock, OnceLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use crate::pty_types::{CommandBuilder, ExitStatus, PtySize};
use crossbeam_channel::{Receiver, Sender};
use libghostty_vt::{
    RenderState, Terminal,
    alloc::{Allocator, Bytes},
    fmt::{Format, Formatter, FormatterOptions},
    focus, key,
    kitty::graphics::{self, DecodedImage, ImageFormat, PlacementIterator},
    mouse::{self, EncoderSize},
    render::{CellIterator, CellsCopy, Clip, Colors, CursorVisualStyle, Dirty, RowIterator},
    screen::{
        CellContentTag, CellSemanticContent, CellWide, RowSemanticPrompt, Screen, TrackedGridRef,
    },
    selection::{FormatOptions, SelectLineOptions, SelectWordOptions, Selection},
    style::{Palette, RgbColor, StyleColor, Underline},
    terminal::{
        ClipboardContent, ClipboardLocation, ClipboardWriteError, ColorScheme, CompressionMode,
        ConformanceLevel, CursorStyle as GhosttyCursorStyle, DeviceAttributeFeature,
        DeviceAttributes, DeviceType, GridRead, Mode, ModeKind, Point, PointCoordinate, PointSpace,
        PrimaryDeviceAttributes, ScreenSnapshot, ScrollViewport, SecondaryDeviceAttributes,
        SizeReportSize, TertiaryDeviceAttributes,
    },
};
use parking_lot::{Mutex, RwLock};
#[cfg(not(unix))]
use portable_pty::native_pty_system;
#[cfg(windows)]
use portable_pty::{CommandBuilder, ExitStatus, PtySize};
use regex::RegexBuilder;
use smallvec::SmallVec;
use thiserror::Error;

use crate::ColourClass;
use crate::program_status::{
    MAX_PROGRAM_STATUS_OSC_BYTES, PROGRAM_STATUS_PREFIX, ProgramStatus, ProgramStatusReport,
    parse_program_status,
};
use crate::{
    ATTR_BLINK, ATTR_BOLD, ATTR_EXPLICIT_RGB, ATTR_FAINT, ATTR_HYPERLINK, ATTR_INVISIBLE,
    ATTR_ITALIC, ATTR_OVERLINE, ATTR_STRIKETHROUGH, CellWidth, ClipboardTarget, Color, CopyJump,
    CopyJumpDirection, CopyModeAction, CopyModeCountPolicy, CopyModeSearch, CopySelectionMode,
    Cursor, CursorBlinkPolicy, CursorStyle, GRAPHEME_TABLE_BIT, IMAGE_PLACEHOLDER_SCHEME,
    KeyAction, KeyCode, KeyInput, KittyLayer, KittyPlacement, MAX_HISTORY_LIMIT,
    MAX_KITTY_IMAGE_BYTES, MAX_KITTY_PLACEMENTS, Modifiers, OVERLAY_RECTANGLE, OverlayKind,
    OverlaySpan, PackedCell, PackedStyle, PasteBufferAction, PointerCellEvent, ScrollbarState,
    SearchCase, SearchDirection, SearchMode, SearchQuery, SearchStatus, SessionStatus,
    TerminalAppearance, TerminalColorScheme, TerminalDictionary, TerminalMode, TerminalMouseButton,
    TerminalMouseInput, TerminalMousePhase, TerminalPresentation, TerminalViewAction,
    TerminalViewId, TerminalViewport, UnderlineStyle, WordSeparators,
};

mod copy_grid;
#[cfg(test)]
mod deferred_event_tests;
mod mode_revision;
mod pane_actor;
#[cfg(all(test, unix))]
mod pane_tests;
#[cfg(test)]
mod reader_tests;
mod shard;
mod surface_actor;
#[cfg(unix)]
mod unix_pty;

thread_local! {
    static ROUND_TRIPS_FORBIDDEN: Cell<bool> = const { Cell::new(false) };
}

#[must_use]
pub fn forbid_actor_round_trips() -> RoundTripGuard {
    RoundTripGuard(ROUND_TRIPS_FORBIDDEN.with(|forbidden| forbidden.replace(true)))
}

#[must_use]
pub fn run_pty_exec_mode() -> Option<std::process::ExitCode> {
    #[cfg(unix)]
    {
        unix_pty::run_pty_exec_mode()
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
pub fn pin_pty_exec_helper(path: &std::path::Path) {
    unix_pty::pin_pty_exec_helper(path);
}

#[cfg(target_os = "linux")]
pub extern "C" fn disable_transparent_huge_pages() {
    unix_pty::disable_transparent_huge_pages();
}

#[must_use]
pub fn allow_actor_round_trips() -> RoundTripGuard {
    RoundTripGuard(ROUND_TRIPS_FORBIDDEN.with(|forbidden| forbidden.replace(false)))
}

pub struct RoundTripGuard(bool);

impl Drop for RoundTripGuard {
    fn drop(&mut self) {
        ROUND_TRIPS_FORBIDDEN.with(|forbidden| forbidden.set(self.0));
    }
}

#[cfg(unix)]
type HeldWake = (Arc<std::os::fd::OwnedFd>, Option<Arc<AtomicBool>>);

#[cfg(unix)]
thread_local! {
    static HELD_WAKES: RefCell<Option<Vec<HeldWake>>> = const { RefCell::new(None) };
}

#[must_use]
pub fn hold_actor_wakes() -> WakeHold {
    #[cfg(unix)]
    {
        WakeHold(
            HELD_WAKES.with_borrow_mut(|held| {
                if held.is_some() {
                    return false;
                }
                *held = Some(Vec::new());
                true
            }),
            std::marker::PhantomData,
        )
    }
    #[cfg(not(unix))]
    WakeHold(std::marker::PhantomData)
}

pub struct WakeHold(#[cfg(unix)] bool, std::marker::PhantomData<*const ()>);

#[cfg(unix)]
impl Drop for WakeHold {
    fn drop(&mut self) {
        if self.0 {
            let held = HELD_WAKES.with_borrow_mut(Option::take);
            write_held_wakes(held.unwrap_or_default());
        }
    }
}

#[cfg(unix)]
fn hold_wake(pipe: &Arc<std::os::fd::OwnedFd>, pending: Option<&Arc<AtomicBool>>) -> bool {
    HELD_WAKES
        .try_with(|held| {
            let mut held = held.borrow_mut();
            let Some(held) = held.as_mut() else {
                return false;
            };
            if !held.iter().any(|(seen, _)| Arc::ptr_eq(seen, pipe)) {
                held.push((Arc::clone(pipe), pending.cloned()));
            }
            true
        })
        .unwrap_or(false)
}

#[cfg(unix)]
pub fn release_held_wakes() {
    let held = HELD_WAKES
        .try_with(|held| held.borrow_mut().as_mut().map(std::mem::take))
        .ok()
        .flatten();
    write_held_wakes(held.unwrap_or_default());
}

#[cfg(unix)]
fn write_held_wakes(held: Vec<HeldWake>) {
    for (pipe, pending) in held {
        wake_pipe(&pipe, pending.as_ref());
    }
}

#[cfg(unix)]
fn wake_pipe(pipe: &std::os::fd::OwnedFd, pending: Option<&Arc<AtomicBool>>) {
    if pending.is_some_and(|pending| pending.swap(true, Ordering::AcqRel)) {
        return;
    }
    match write_actor_wake(|| rustix::io::write(pipe, &[1_u8])) {
        Ok(()) | Err(rustix::io::Errno::PIPE) => {}
        Err(error) => log::error!("failed to wake terminal actor: {error}"),
    }
}

#[cfg(all(test, unix))]
#[path = "session/wake_hold_tests.rs"]
mod wake_hold_tests;

use mode_revision::{ModeRevision, ModeSelection};
use pane_actor::PaneActor;
mod view_state;
use view_state::*;
mod actor_channels;
use actor_channels::*;
mod engine_filter;
mod handle;
use engine_filter::*;

const INITIAL_COLUMNS: u16 = 80;
const INITIAL_ROWS: u16 = 24;
const EMPTY_PANE_MODES: &str = "\x1b[20h\x1b[?25l";
const INITIAL_CELL_WIDTH: u32 = 8;
const INITIAL_CELL_HEIGHT: u32 = 18;
const MAX_LINK_URI_BYTES: usize = 16 * 1024;
const LINK_URI_SCRATCH_BYTES: usize = 256;
const GRAPHEME_CLUSTER_SCRATCH: usize = 32;
const MAX_GRAPHEME_CLUSTER_CODEPOINTS: usize = 256;
const IMAGE_PLACEHOLDERS: [(&str, &str); 1] = [("[Image #", "]")];
const MAX_PLACEHOLDER_CELLS: usize = 32;
const MAX_CAPTURE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_LAST_COMMAND_LINES: usize = 200;
pub const MAX_LAST_COMMAND_BYTES: usize = 256 * 1024;
const MAX_OUTPUT_VIEW_SCROLLBACK: usize = 100_000;
const MAX_STARTUP_OUTPUT_VIEW_SCROLLBACK: usize = 64 * 1024 * 1024;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);
const SEARCH_REFRESH_DEBOUNCE: Duration = Duration::from_millis(80);
const TERMINATION_GRACE: Duration = Duration::from_millis(500);
const TERMINATION_KILL_WAIT: Duration = Duration::from_millis(500);
#[cfg(unix)]
const PANE_EXEC_WAIT: Duration = Duration::from_millis(500);
#[cfg(target_os = "macos")]
const SHARD_CHILD_RETRY: Duration = Duration::from_millis(10);
const MAX_SEARCH_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
const MAX_WHEEL_REPEAT: u32 = 32;
const PTY_READ_BUFFER_BYTES: usize = 64 * 1024;
#[cfg(target_os = "linux")]
const PTY_BUFFER_POOL_SIZE: usize = 4;
#[cfg(all(not(target_os = "linux"), any(not(unix), test)))]
const PTY_BUFFER_POOL_SIZE: usize = 8;
const PTY_DRAIN_TURN_BYTES: usize = 256 * 1024;
const PTY_DRAIN_TURN_TIME: Duration = Duration::from_millis(1);
#[cfg(unix)]
const PTY_BRIDGE_THRESHOLD_BYTES: usize = 1024;
#[cfg(all(unix, not(target_os = "linux")))]
const PTY_BRIDGE_SPIN_MAX: u32 = 512;
#[cfg(target_os = "linux")]
const PTY_BRIDGE_SPIN_MAX: u32 = PTY_GATHER_BRIDGE_SPIN_MAX;
#[cfg(unix)]
const PTY_BRIDGE_SPIN_MIN: u32 = 8;
#[cfg(target_os = "linux")]
const PTY_GATHER_BRIDGE_SPIN_MAX: u32 = 16;
#[cfg(target_os = "linux")]
const PTY_GATHER_ADMIT_PASSES: u32 = 8;
#[cfg(target_os = "linux")]
const PTY_GATHER_LEND_BUSY: Duration = Duration::from_millis(10);
#[cfg(target_os = "linux")]
const PTY_GATHER_LEND_IDLE: Duration = Duration::from_millis(100);
#[cfg(target_os = "linux")]
const PTY_GATHER_LEND_LINGER: Duration = Duration::from_secs(1);
#[cfg(target_os = "linux")]
const PTY_GATHER_LENT_MAX: usize = 3;
const CONTENT_PUBLISH_STALENESS: Duration = Duration::from_millis(16);
const SYNCHRONIZED_OUTPUT_TIMEOUT: Duration = Duration::from_secs(1);
#[cfg(unix)]
const PTY_WRITE_RETRY: Duration = Duration::from_millis(16);
#[cfg(unix)]
const PTY_WRITE_BUDGET_BYTES: usize = 1024 * 1024;
#[cfg(unix)]
const PTY_WRITE_RETAIN_BYTES: usize = 64 * 1024;
const MAX_PENDING_PTY_INPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_PENDING_PTY_INPUT_COMMANDS: usize = 256;
const PTY_INPUT_COMMAND_FLOOR_BYTES: usize = 4 * 1024;
const PTY_INPUT_OVERFLOW_ENTRY_BYTES: usize = 256;
const PENDING_PASTE_WINDOW: Duration = Duration::from_secs(5);
const IDLE_SLEEP: Duration = Duration::from_hours(1);
const MAX_PTY_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_CLIPBOARD_WRITE_BYTES: usize = 8 * 1024 * 1024;
const CLIPBOARD_TEXT_MIME: &str = "text/plain";
const MAX_PENDING_ACTOR_COMMANDS: usize = 1;
const MAX_PENDING_RELIABLE_EVENTS: usize = 4;
const MAX_PENDING_SHELL_MARKS: usize = 65_536;
const MAX_PENDING_RELIABLE_EVENT_BYTES: usize = 64 * 1024 * 1024;
const MAX_PENDING_TERMINAL_EVENTS: usize = MAX_PENDING_RELIABLE_EVENTS + 1;
const RETAINED_CELL_PLANES: usize = 2;
const RETAINED_OVERLAY_PLANES: usize = 2;
const MAX_VIEWPORT_STYLE_COUNT: usize = 65_536;
const MAX_VIEWPORT_GRAPHEME_COUNT: usize = 1024 * 1024;
const MAX_VIEWPORT_GRAPHEME_BYTES: usize = 16 * 1024 * 1024;
const MIN_STYLE_COMPACTION_LIMIT: usize = 4 * 1024;
const MIN_GRAPHEME_COMPACTION_LIMIT: usize = 16 * 1024;
const MIN_GRAPHEME_BYTE_COMPACTION_LIMIT: usize = 1024 * 1024;
const TMUX_PASSTHROUGH_PREFIX: &[u8] = b"\x1bPtmux;";
const MAX_TMUX_PASSTHROUGH_PAYLOAD_BYTES: usize = 1024 * 1024;

const DEAD_NOTICE_WAIT: Duration = Duration::from_secs(5);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProgressBarState {
    #[default]
    Hidden,
    Normal,
    Error,
    Indeterminate,
    Paused,
}

impl ProgressBarState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Normal => "normal",
            Self::Error => "error",
            Self::Indeterminate => "indeterminate",
            Self::Paused => "paused",
        }
    }

    const fn from_digit(digit: u8) -> Option<Self> {
        Some(match digit {
            b'0' => Self::Hidden,
            b'1' => Self::Normal,
            b'2' => Self::Error,
            b'3' => Self::Indeterminate,
            b'4' => Self::Paused,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProgressBar {
    pub state: ProgressBarState,
    pub progress: u8,
}

impl ProgressBar {
    fn apply(&mut self, state: ProgressBarState, progress: Option<u8>) {
        self.state = state;
        if let Some(progress) = progress
            && state != ProgressBarState::Indeterminate
        {
            self.progress = progress;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineKnobs {
    pub scroll_on_clear: bool,
    pub alternate_screen: bool,
    pub allow_rename: bool,
    pub erase_byte: Option<u8>,
    pub verase_byte: u8,
    pub mode_keys_vi: bool,
}

impl Default for EngineKnobs {
    fn default() -> Self {
        Self {
            scroll_on_clear: true,
            alternate_screen: true,
            allow_rename: false,
            erase_byte: Some(0x7f),
            verase_byte: 0x7f,
            mode_keys_vi: false,
        }
    }
}

#[derive(Default)]
struct EngineFilter {
    state: EngineState,
    sequence: Vec<u8>,
    title: Vec<u8>,
    osc: Vec<u8>,
    osc_dropped: usize,
    osc_overflowed: bool,
    integration_title: bool,
    program_title_writes: u64,
    bar: ProgressBar,
    primary_history_size: usize,
    metadata_hint: bool,
    program_status: ProgramStatus,
    program_status_changed: bool,
    replies: Option<Rc<RefCell<PtyEffects>>>,
    output: PaneOutputFacts,
    shell_marks: Vec<ShellMark>,
    cursor_blink_set: bool,
    mouse_mode: Option<usize>,
    output_marks: Vec<(Screen, TrackedGridRef)>,
    shell_starting: bool,
}

pub const PRIVATE_MODE_NUMBERS: [u16; 14] = [
    1, 6, 7, 12, 25, 1000, 1002, 1003, 1004, 1005, 1006, 2004, 2026, 2031,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellMarkKind {
    Prompt,
    CommandStarted,
    CommandFinished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellMark {
    pub kind: ShellMarkKind,
    pub facts: PaneOutputFacts,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PaneOutputFacts {
    pub output_generation: u64,
    pub last_output_time: u64,
    pub last_prompt_time: u64,
    pub command_start_time: u64,
    pub command_end_time: u64,
    pub command_running: bool,
    pub command_status: Option<u8>,
}

impl PaneOutputFacts {
    fn output(&mut self, now: u64) {
        self.output_generation += 1;
        self.last_output_time = now;
    }

    fn osc_133(&mut self, mark: &[u8], now: u64) {
        match mark.first() {
            Some(b'A' | b'N') => self.last_prompt_time = now,
            Some(b'C') => {
                self.command_start_time = now;
                self.command_end_time = 0;
                self.command_running = true;
                self.command_status = None;
            }
            Some(b'D') => {
                self.command_end_time = now;
                self.command_running = false;
                self.command_status = Some(osc_133_exit_status(mark));
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn command_duration(&self, now: u64) -> Option<u64> {
        if self.command_start_time == 0 {
            return None;
        }
        let end = if self.command_running {
            now
        } else {
            self.command_end_time
        };
        Some(end.max(self.command_start_time) - self.command_start_time)
    }
}

#[derive(Default)]
struct PassthroughFilter {
    mode: AllowPassthrough,
    state: PassthroughState,
    payload: Vec<u8>,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WheelRoute {
    ApplicationMouse,
    AlternateScroll,
    Viewport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureBoundary {
    HistoryStart,
    VisibleEnd,
    Relative(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "these independent switches directly model orthogonal capture-pane flags"
)]
pub struct CaptureOptions {
    pub start: CaptureBoundary,
    pub end: CaptureBoundary,
    pub alternate: bool,
    pub mode: bool,
    pub join_wrapped: bool,
    pub preserve_trailing: bool,
    pub trim_positions: bool,
    pub escape_sequences: bool,
    pub escape_nonprintable: bool,
    pub number_lines: bool,
    pub line_flags: bool,
    pub hyperlinks: bool,
    pub skip_input: bool,
    pub keep_idle_prompt: bool,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            start: CaptureBoundary::Relative(0),
            end: CaptureBoundary::VisibleEnd,
            alternate: false,
            mode: false,
            join_wrapped: false,
            preserve_trailing: false,
            trim_positions: false,
            escape_sequences: false,
            escape_nonprintable: false,
            number_lines: false,
            line_flags: false,
            hyperlinks: false,
            skip_input: false,
            keep_idle_prompt: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LastCommandCapture {
    pub command: String,
    pub output: String,
    pub truncated_rows: usize,
    pub running: bool,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TerminalCaptureError {
    #[error("terminal actor stopped")]
    ActorStopped,
    #[error("terminal capture timed out")]
    TimedOut,
    #[error("no alternate screen")]
    AlternateUnavailable,
    #[error("pane is not in a native mode")]
    ModeUnavailable,
    #[error("no shell-integration marks in this pane's scrollback")]
    NoSemanticMarks,
    #[error("terminal capture exceeds the {MAX_CAPTURE_BYTES}-byte limit")]
    TooLarge,
    #[error("terminal capture failed: {0}")]
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalCopyReady {
    pub request_id: u64,
    pub clipboard: Option<ClipboardTarget>,
    pub buffer: Option<PasteBufferAction>,
    pub pipe: Option<String>,
    pub text: String,
    view_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalOpenUri {
    pub view: TerminalViewId,
    pub uri: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    ViewportReady {
        output_activity: bool,
    },
    ViewClosed(TerminalViewId),
    CopyReady {
        view: TerminalViewId,
        copy: Box<TerminalCopyReady>,
    },
    OpenUri(Box<TerminalOpenUri>),
    ClipboardSet {
        target: ClipboardTarget,
        text: String,
    },
    RenameWindow(String),
    Bell,
    PlaceholderBound {
        token: u64,
        number: u32,
    },
    PendingPasteExpired {
        token: u64,
    },
}

pub type ViewFrame = (TerminalViewId, Arc<TerminalViewport>, Option<u64>);

pub trait TerminalFrameSink: Send + Sync {
    fn deliver(&self, frames: &[ViewFrame], sunk: &mut Vec<TerminalViewId>) -> bool;

    fn published(&self, notified: bool);

    fn takes_output(&self) -> bool {
        false
    }

    fn output_room(&self) -> usize {
        usize::MAX
    }

    fn output(&self, _bytes: &Arc<[u8]>) {}

    fn as_any(&self) -> &dyn std::any::Any;
}

#[cfg(test)]
pub(crate) struct TestOutputSink {
    taking: AtomicBool,
    room: AtomicUsize,
    sender: Sender<Arc<[u8]>>,
}

#[cfg(test)]
impl TestOutputSink {
    pub(crate) fn install(session: &TerminalSession) -> (Arc<Self>, Receiver<Arc<[u8]>>) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let sink = Arc::new(Self {
            taking: AtomicBool::new(true),
            room: AtomicUsize::new(usize::MAX),
            sender,
        });
        session.install_frame_sink(Arc::clone(&sink) as Arc<dyn TerminalFrameSink>);
        (sink, receiver)
    }

    pub(crate) fn take(&self, taking: bool) {
        self.taking.store(taking, Ordering::Release);
    }

    pub(crate) fn set_room(&self, room: usize) {
        self.room.store(room, Ordering::Release);
    }
}

#[cfg(test)]
impl TerminalFrameSink for TestOutputSink {
    fn deliver(&self, _: &[ViewFrame], _: &mut Vec<TerminalViewId>) -> bool {
        false
    }

    fn published(&self, _: bool) {}

    fn takes_output(&self) -> bool {
        self.taking.load(Ordering::Acquire)
    }

    fn output_room(&self) -> usize {
        self.room.load(Ordering::Acquire)
    }

    fn output(&self, bytes: &Arc<[u8]>) {
        let _ = self
            .room
            .try_update(Ordering::AcqRel, Ordering::Acquire, |room| {
                Some(room.saturating_sub(1))
            });
        let _ = self.sender.send(Arc::clone(bytes));
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone)]
pub struct OutputWake(ActorWake);

impl OutputWake {
    pub fn wake(&self) {
        self.0.notify();
    }
}

#[derive(Clone)]
pub struct TerminalEvents {
    receiver: async_channel::Receiver<TerminalEvent>,
    state: Arc<EventQueueState>,
}

struct ForegroundSource {
    #[cfg(unix)]
    master: Arc<unix_pty::UnixMaster>,
    shell: Option<u32>,
    tty: Option<PathBuf>,
}

impl ForegroundSource {
    fn process_id(&self) -> Option<u32> {
        #[cfg(unix)]
        {
            self.master
                .process_group_leader()
                .and_then(|group| u32::try_from(group).ok())
                .filter(|process_id| *process_id != 0)
                .or_else(|| self.shell_process_id())
        }
        #[cfg(not(unix))]
        {
            self.shell_process_id()
        }
    }

    fn shell_process_id(&self) -> Option<u32> {
        self.shell.filter(|process_id| *process_id != 0)
    }

    fn canonical(&self) -> Option<bool> {
        #[cfg(unix)]
        {
            self.master.canonical()
        }
        #[cfg(not(unix))]
        {
            None
        }
    }
}

static OUTPUT_CLOCK: LazyLock<Instant> = LazyLock::new(Instant::now);

#[repr(C)]
struct EventQueueState {
    pending_reliable: AtomicUsize,
    pending_reliable_bytes: AtomicUsize,
    notification_pending: AtomicBool,
    output_activity_pending: AtomicBool,
    output_since_check: AtomicBool,
    last_output: AtomicU32,
    identity: Box<IdentityLatch>,
    foreground: RwLock<Option<Box<ForegroundSource>>>,
    completion: AtomicU64,
    sinks: Box<EventSinks>,
}

#[derive(Default)]
struct EventSinks {
    notification: OnceLock<Box<dyn Fn() + Send + Sync>>,
    frame: OnceLock<Arc<dyn TerminalFrameSink>>,
}

impl EventQueueState {
    fn new() -> Self {
        Self {
            pending_reliable: AtomicUsize::new(0),
            pending_reliable_bytes: AtomicUsize::new(0),
            notification_pending: AtomicBool::new(false),
            output_activity_pending: AtomicBool::new(false),
            output_since_check: AtomicBool::new(false),
            last_output: AtomicU32::new(0),
            identity: Box::new(IdentityLatch::default()),
            foreground: RwLock::new(None),
            completion: AtomicU64::new(0),
            sinks: Box::default(),
        }
    }

    fn notify_consumer(&self) {
        if let Some(sink) = self.sinks.notification.get() {
            sink();
        }
    }

    fn mark_output(&self) {
        let millis = OUTPUT_CLOCK.elapsed().as_millis() as u32;
        self.last_output.store(millis.max(1), Ordering::Release);
        self.output_since_check.store(true, Ordering::Release);
    }

    fn last_output(&self) -> Option<Instant> {
        let stored = self.last_output.load(Ordering::Acquire);
        if stored == 0 {
            return None;
        }
        let now = OUTPUT_CLOCK.elapsed().as_millis() as u64;
        let ago = (now as u32).wrapping_sub(stored);
        Some(*OUTPUT_CLOCK + Duration::from_millis(now.saturating_sub(u64::from(ago))))
    }

    fn take_output_since_check(&self) -> bool {
        self.output_since_check.load(Ordering::Acquire)
            && self.output_since_check.swap(false, Ordering::AcqRel)
    }

    fn resolve_identity(&self) {
        let mut resolved = self.identity.state.lock();
        if !resolved.ready {
            resolved.ready = true;
            self.identity.ready.notify_all();
            let replies = std::mem::take(&mut resolved.replies);
            drop(resolved);
            for reply in replies {
                let _ = reply.send(true);
            }
        }
    }
}

impl EventQueueState {
    fn received(&self, event: &mut TerminalEvent) {
        if let TerminalEvent::ViewportReady { output_activity } = event {
            self.notification_pending.store(false, Ordering::Release);
            *output_activity = self.output_activity_pending.swap(false, Ordering::AcqRel);
            return;
        }

        let previous = self.pending_reliable.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "reliable terminal event accounting underflow");
        let bytes = reliable_event_bytes(event);
        let previous_bytes = self
            .pending_reliable_bytes
            .fetch_sub(bytes, Ordering::AcqRel);
        debug_assert!(
            previous_bytes >= bytes,
            "reliable terminal event byte accounting underflow"
        );
    }
}

pub struct DeferredTerminalEvent {
    event: Option<TerminalEvent>,
    state: Arc<EventQueueState>,
}

impl DeferredTerminalEvent {
    pub fn event(&self) -> &TerminalEvent {
        self.event.as_ref().expect("unconsumed terminal event")
    }

    pub fn into_event(mut self) -> TerminalEvent {
        let mut event = self.event.take().expect("unconsumed terminal event");
        self.state.received(&mut event);
        event
    }
}

impl Drop for DeferredTerminalEvent {
    fn drop(&mut self) {
        if let Some(mut event) = self.event.take() {
            self.state.received(&mut event);
        }
    }
}

impl TerminalEvents {
    pub fn install_notification_sink(&self, sink: impl Fn() + Send + Sync + 'static) {
        assert!(self.state.sinks.notification.set(Box::new(sink)).is_ok());
        self.state.notify_consumer();
    }

    pub fn try_recv_deferred(&self) -> Result<DeferredTerminalEvent, async_channel::TryRecvError> {
        Ok(DeferredTerminalEvent {
            event: Some(self.receiver.try_recv()?),
            state: Arc::clone(&self.state),
        })
    }

    pub fn recv_deferred_blocking(
        &self,
    ) -> Result<DeferredTerminalEvent, async_channel::RecvError> {
        Ok(DeferredTerminalEvent {
            event: Some(self.receiver.recv_blocking()?),
            state: Arc::clone(&self.state),
        })
    }

    pub fn recv_blocking(&self) -> Result<TerminalEvent, async_channel::RecvError> {
        let mut event = self.receiver.recv_blocking()?;
        self.state.received(&mut event);
        Ok(event)
    }

    pub fn try_recv(&self) -> Result<TerminalEvent, async_channel::TryRecvError> {
        let mut event = self.receiver.try_recv()?;
        self.state.received(&mut event);
        Ok(event)
    }
}

#[derive(Clone)]
struct TerminalEventSender(Arc<TerminalEventProducer>);

struct TerminalEventProducer {
    sender: async_channel::Sender<TerminalEvent>,
    state: Arc<EventQueueState>,
}

impl std::ops::Deref for TerminalEventSender {
    type Target = async_channel::Sender<TerminalEvent>;

    fn deref(&self) -> &Self::Target {
        &self.0.sender
    }
}

impl Drop for TerminalEventProducer {
    fn drop(&mut self) {
        self.sender.close();
        self.state.notify_consumer();
    }
}

fn terminal_event_channel(state: &Arc<EventQueueState>) -> (TerminalEventSender, TerminalEvents) {
    let (sender, receiver) = async_channel::bounded(MAX_PENDING_TERMINAL_EVENTS);
    let events = TerminalEvents {
        receiver,
        state: Arc::clone(state),
    };
    (
        TerminalEventSender(Arc::new(TerminalEventProducer {
            sender,
            state: Arc::clone(state),
        })),
        events,
    )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalFacts {
    pub output: PaneOutputFacts,
    pub private_modes: u16,
    pub history_size: usize,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub alternate_on: bool,
    pub mouse_tracking: bool,
    pub cursor_hidden: bool,
    pub program_title_writes: u64,
}

struct PublishedViewports {
    fallback: Arc<TerminalViewport>,
    by_view: HashMap<TerminalViewId, Arc<TerminalViewport>>,
    epochs: HashMap<TerminalViewId, u64>,
    fallback_current: bool,
    copy_facts: HashMap<TerminalViewId, Arc<CopyModeFacts>>,
    bar: ProgressBar,
    program_status: Arc<ProgramStatus>,
    last_command_status: Option<i32>,
    facts: TerminalFacts,
    search_string: String,
    sunk: Vec<TerminalViewId>,
    size: (u16, u16),
    shell_marks: VecDeque<ShellMark>,
    shell_marks_dropped: u64,
}

impl PublishedViewports {
    fn new(viewport: TerminalViewport) -> Self {
        Self {
            size: (viewport.columns, viewport.rows),
            fallback: Arc::new(viewport),
            by_view: HashMap::new(),
            epochs: HashMap::new(),
            fallback_current: false,
            copy_facts: HashMap::new(),
            bar: ProgressBar::default(),
            program_status: Arc::default(),
            last_command_status: None,
            facts: TerminalFacts::default(),
            search_string: String::new(),
            sunk: Vec::new(),
            shell_marks: VecDeque::new(),
            shell_marks_dropped: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyModeFacts {
    pub view_mode: bool,
    pub cursor_x: u32,
    pub cursor_y: u32,
    pub cursor_line: String,
    pub cursor_word: String,
    pub scroll_position: u32,
    pub selection: Option<CopyModeSelectionFacts>,
    pub search_present: bool,
    pub search_count: Option<(u32, bool)>,
    pub search_timed_out: bool,
    pub search_match: String,
    pub rectangle_toggle: bool,
    pub selection_active: bool,
    pub line_numbers: u8,
    pub refresh_active: bool,
}

#[must_use]
pub fn copy_line_number_mode(line_numbers: u8, option: &str) -> u8 {
    if line_numbers == 0 {
        return 0;
    }
    let mode = match option {
        "default" => 1,
        "absolute" => 2,
        "relative" => 3,
        "hybrid" => 4,
        _ => 0,
    };
    if line_numbers == 2 && mode == 0 {
        1
    } else {
        mode
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CopyModeSelectionFacts {
    pub start_x: u32,
    pub start_y: u32,
    pub end_x: u32,
    pub end_y: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
}

impl TerminalSize {
    #[must_use]
    pub fn cells(columns: u16, rows: u16) -> Self {
        Self {
            columns,
            rows,
            cell_width_px: INITIAL_CELL_WIDTH,
            cell_height_px: INITIAL_CELL_HEIGHT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalProcessExit {
    pub code: u32,
    pub signal: Option<u8>,
}

impl TerminalProcessExit {
    const PRESENT: u64 = 1 << 63;

    fn encode(self) -> u64 {
        Self::PRESENT | u64::from(self.code) | (self.signal.map_or(0, u64::from) << u32::BITS)
    }

    fn decode(value: u64) -> Option<Self> {
        if value & Self::PRESENT == 0 {
            return None;
        }
        let code = u32::try_from(value & u64::from(u32::MAX)).expect("masked to u32");
        let signal = u8::try_from((value >> u32::BITS) & u64::from(u8::MAX)).expect("masked to u8");
        Some(Self {
            code,
            signal: (signal != 0).then_some(signal),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TerminalSpawn {
    pub knobs: EngineKnobs,
    pub working_directory: Option<PathBuf>,
    pub command: Option<Vec<String>>,
    pub shell: Option<String>,
    pub terminal_type: Option<String>,
    pub initial_size: Option<TerminalSize>,
    pub non_login_shell: bool,
    pub env: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)>,
    pub word_separators: Option<WordSeparators>,
    pub allow_passthrough: Option<bool>,
    pub wrap_search: Option<bool>,
}

pub struct TerminalSession {
    commands: CommandSender,
    events: TerminalEvents,
    latest: Arc<RwLock<PublishedViewports>>,
    max_scrollback: usize,
    word_separators: RwLock<WordSeparators>,
    applied_appearance: AtomicU64,
    terminating: AtomicBool,
    preview_pending: AtomicBool,
    retired: AtomicBool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSessionDiagnostics {
    pub command_queue_len: usize,
    pub command_queue_capacity: Option<usize>,
    pub pending_pty_input_bytes: usize,
    pub event_queue_len: usize,
    pub event_queue_capacity: Option<usize>,
    pub pending_reliable_events: usize,
    pub pending_reliable_bytes: usize,
    pub viewport_notification_pending: bool,
    pub max_scrollback: usize,
    pub viewport_generation: u64,
    pub viewport_view_generation: u64,
    pub viewport_dictionary_generation: u32,
    pub viewport_columns: u16,
    pub viewport_rows: u16,
    pub viewport_cells: usize,
    pub viewport_cell_bytes: usize,
    pub viewport_overlays: usize,
    pub viewport_strong_count: usize,
    pub viewport_cell_arc_strong_count: usize,
    pub viewport_dictionary_arc_strong_count: usize,
    pub viewport_overlay_arc_strong_count: usize,
    pub viewport_styles: usize,
    pub viewport_graphemes: usize,
    pub viewport_grapheme_bytes: usize,
    pub viewport_title: Arc<str>,
    pub viewport_status: SessionStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KittyImage {
    pub image_id: u32,
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum KittyImageRequestError {
    #[error("terminal actor did not answer the Kitty image request in time")]
    TimedOut,
    #[error("terminal actor stopped before answering the Kitty image request")]
    ActorStopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum TerminalRequestError {
    #[error("terminal actor request timed out")]
    TimedOut,
    #[error("terminal actor stopped")]
    ActorStopped,
}

type ActorRequestError = TerminalRequestError;

impl From<ActorRequestError> for TerminalCaptureError {
    fn from(error: ActorRequestError) -> Self {
        match error {
            ActorRequestError::TimedOut => Self::TimedOut,
            ActorRequestError::ActorStopped => Self::ActorStopped,
        }
    }
}

impl From<ActorRequestError> for KittyImageRequestError {
    fn from(error: ActorRequestError) -> Self {
        match error {
            ActorRequestError::TimedOut => Self::TimedOut,
            ActorRequestError::ActorStopped => Self::ActorStopped,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Geometry {
    columns: u16,
    rows: u16,
    cell_width_px: u32,
    cell_height_px: u32,
}

#[derive(Debug)]
struct CaptureRequest {
    options: CaptureOptions,
    reply: ActorReply<Result<String, TerminalCaptureError>>,
}

#[derive(Debug)]
struct PointerContextRequest {
    view: TerminalViewId,
    column: u16,
    row: u16,
    reply: ActorReply<PointerContext>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PointerContext {
    pub word: String,
    pub line: String,
    pub hyperlink: String,
}

#[derive(Debug)]
struct LastCommandRequest {
    reply: ActorReply<Result<LastCommandCapture, TerminalCaptureError>>,
}

#[derive(Debug)]
struct KittyImageRequest {
    image_id: u32,
    reply: ActorReply<Option<KittyImage>>,
}

#[derive(Debug)]
struct KittyImageGenerationRequest {
    image_id: u32,
    reply: ActorReply<Option<u64>>,
}

pub type HistoryCapture = (
    u32,
    Vec<Vec<PackedCell>>,
    TerminalDictionary,
    ScrollbarState,
    u16,
);

#[derive(Debug)]
struct HistoryCommand {
    start: u32,
    count: u32,
    reply: ActorReply<Result<HistoryCapture, TerminalCaptureError>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewStream {
    #[default]
    Off,
    Foreground,
    Preview,
}

impl ViewStream {
    const fn is_on(self) -> bool {
        !matches!(self, Self::Off)
    }
}

#[derive(Default)]
struct ControlSlot {
    raw_input_space_waker: Option<std::task::Waker>,
    pending: PendingControl,
    deferred: Vec<(usize, Command)>,
    in_flight: usize,
    known_views: HashSet<TerminalViewId>,
    view_areas: HashMap<TerminalViewId, (u16, u16)>,
    requested_geometry: Option<Geometry>,
    wake_queued: bool,
}

#[derive(Default)]
struct PendingControl {
    word_separators: Option<Box<WordSeparators>>,
    allow_passthrough: Option<AllowPassthrough>,
    wrap_search: Option<bool>,
    engine_knobs: Option<EngineKnobs>,
    appearance: Option<Arc<TerminalAppearance>>,
    view_areas: Option<HashMap<TerminalViewId, (u16, u16)>>,
    resize: Option<Geometry>,
    views: Vec<Command>,
    preview: Option<bool>,
}

struct QueuedInput {
    command: Command,
    permit: InputPermit,
}

struct InputReceiver {
    commands: Receiver<QueuedInput>,
    admission: Arc<Mutex<InputAdmission>>,
    #[cfg(unix)]
    direct: Arc<DirectInput>,
}

struct InputSender {
    commands: Sender<QueuedInput>,
    admission: Arc<Mutex<InputAdmission>>,
    max_commands: usize,
    max_bytes: usize,
    #[cfg(unix)]
    direct: Arc<DirectInput>,
}

#[derive(Clone)]
struct ActorWake {
    ready: Option<Arc<AtomicBool>>,
    #[cfg(unix)]
    pipe: Option<Arc<std::os::fd::OwnedFd>>,
    #[cfg(unix)]
    pending: Option<Arc<AtomicBool>>,
    #[cfg(not(unix))]
    channel: Option<Sender<()>>,
}

#[derive(Default)]
struct IdentityLatch {
    state: Mutex<IdentityState>,
    ready: parking_lot::Condvar,
}

#[derive(Default)]
struct IdentityState {
    ready: bool,
    replies: Vec<ActorReply<bool>>,
}

pub struct TerminalRequest<T> {
    response: Receiver<Result<T, TerminalRequestError>>,
    deadline: Instant,
    queues: Arc<CommandQueues>,
}

impl<T> TerminalRequest<T> {
    pub fn next_poll(&self) -> Instant {
        self.deadline
    }

    pub fn poll(&mut self, now: Instant) -> Option<Result<T, TerminalRequestError>> {
        if let Ok(result) = self.response.try_recv() {
            return Some(result);
        }
        if now >= self.deadline {
            return Some(Err(TerminalRequestError::TimedOut));
        }
        if let Err(crossbeam_channel::TryRecvError::Disconnected) = self.queues.liveness.try_recv()
        {
            return Some(
                self.response
                    .try_recv()
                    .unwrap_or(Err(TerminalRequestError::ActorStopped)),
            );
        }
        match self.response.try_recv() {
            Ok(result) => Some(result),
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                Some(Err(TerminalRequestError::ActorStopped))
            }
            Err(crossbeam_channel::TryRecvError::Empty) => None,
        }
    }
}

struct CommandQueues {
    control: Sender<Command>,
    input: Option<InputSender>,
    liveness: Receiver<Infallible>,
    slot: Arc<Mutex<ControlSlot>>,
    wake: ActorWake,
}

struct CommandSender {
    queues: Arc<CommandQueues>,
}

#[cfg(test)]
#[path = "session/request_tests.rs"]
mod request_tests;

#[derive(Debug)]
struct SelectionState {
    anchor: TrackedGridRef,
    focus: TrackedGridRef,
    mode: SelectionMode,
    rectangle: bool,
}

#[derive(Debug)]
struct CopyModeState {
    revision: Arc<ModeRevision>,
    cursor: PointCoordinate,
    viewport_offset: u32,
    scroll_exit: bool,
    hide_position: bool,
    selection: Option<ModeSelection>,
    selection_mode: CopySelectionMode,
    selection_origin: PointCoordinate,
    recentre: Option<(u32, RecentreTarget)>,
    mark: Option<PointCoordinate>,
    last_jump: Option<CopyJump>,
    selecting: bool,
    rectangle: bool,
    refresh: bool,
    sourced: bool,
    kind: FrozenModeKind,
    last_cx: u16,
    last_sx: u16,
    mode_keys_vi_at_entry: bool,
    search_marks: bool,
    search_count: Option<(u32, bool)>,
    incremental_origin: Option<CopyModeIncrementalOrigin>,
    search: Option<CopyModeSearch>,
    search_all: bool,
    line_numbers: u8,
}

#[derive(Clone, Copy, Debug)]
struct CopyModeIncrementalOrigin {
    row: u32,
    viewport_offset: u32,
}

#[derive(Debug)]
pub struct CapturedCopySource {
    revision: Arc<ModeRevision>,
    cursor: PointCoordinate,
    viewport_offset: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HoverLink {
    row: u16,
    start: u16,
    end: u16,
    uri: String,
}

#[derive(Default)]
struct TerminalScreenViewState {
    viewport: ViewportAnchor,
    selection: Option<SelectionState>,
    copy_mode: CopyModeSlot,
    search: SearchSlot,
    search_origin: Option<PointCoordinate>,
    search_snapshot: Option<Arc<HistorySearchSnapshot>>,
    hover_link: Option<HoverLink>,
    unseen_output: u32,
    mouse_button_pressed: bool,
}

struct TerminalViewState {
    screen: Screen,
    primary: TerminalScreenViewState,
    alternate: TerminalScreenViewState,
}

#[derive(Debug, Default)]
struct ViewportDictionary {
    generation: u32,
    last_style_id: u16,
    mode_revision: Option<u64>,
    mode_viewport: Option<(u64, u32, u32, (u16, u16))>,
    default_style: Option<PackedStyle>,
    palette: Option<Box<[RgbColor; 256]>>,
    class_hints: ClassHints,
    styles: Vec<PackedStyle>,
    style_ids: InternHashMap<PackedStyle, u16>,
    grapheme_ids: InternHashMap<String, u32>,
    grapheme_offsets: Vec<u32>,
    grapheme_bytes: Vec<u8>,
    shared_dictionary: Arc<TerminalDictionary>,
    shared_cells: Arc<[PackedCell]>,
    shared_columns: u16,
    cell_pool: SmallVec<[Arc<[PackedCell]>; RETAINED_CELL_PLANES]>,
    shared_presentation: Arc<TerminalPresentation>,
    shared_overlays: Arc<[OverlaySpan]>,
    overlay_pool: SmallVec<[Arc<[OverlaySpan]>; RETAINED_OVERLAY_PLANES]>,
    row_copy: CellsCopy,
    row_styles: Vec<Option<u16>>,
    overlay_scratch: Vec<OverlaySpan>,
    shared_dirty: u8,
    style_compaction_limit: usize,
    grapheme_compaction_limit: usize,
    grapheme_byte_compaction_limit: usize,
    style_overflowed: bool,
    grapheme_overflowed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SearchMatch {
    row: u32,
    end_row: u32,
    start: u16,
    end: u16,
}

impl SearchMatch {
    fn span(self, row: u32, columns: u16) -> Option<(u16, u16)> {
        if row < self.row || row > self.end_row {
            return None;
        }
        let start = if row == self.row { self.start } else { 0 };
        let end = if row == self.end_row {
            self.end
        } else {
            columns
        };
        Some((start.min(columns), end.min(columns)))
    }

    fn contains(self, point: PointCoordinate, columns: u16) -> bool {
        self.span(point.y, columns)
            .is_some_and(|(start, end)| (start..end).contains(&point.x))
    }
}

#[derive(Debug, Default)]
struct SearchState {
    query: SearchQuery,
    matches: Vec<SearchMatch>,
    current: Option<usize>,
    request_id: u64,
    pending: bool,
    invalid_pattern: bool,
}

fn store_search_state(slot: &mut SearchSlot, state: SearchState) {
    if let Some(current) = slot.as_mut() {
        **current = state;
    } else {
        *slot = Some(Box::new(state));
    }
}

#[derive(Default)]
struct ViewportGenerations {
    content: u64,
    view: u64,
    kitty: Option<KittyGraphicsState>,
}

enum FallbackFrame {
    FirstStreamed,
    Built(TerminalViewport),
    Metadata(TerminalViewport),
}

#[derive(Clone, Copy, Debug)]
enum SnapshotChange {
    Content,
    View,
    Overlay,
}

#[cfg(any(target_os = "linux", not(unix), test))]
enum ReaderMessage {
    Data {
        buffer: Vec<u8>,
        length: usize,
    },
    #[cfg(target_os = "linux")]
    Home,
    Eof,
}

#[derive(Debug)]
struct PtyEffects {
    bytes: Vec<u8>,
    overflowed: bool,
}

impl PtyEffects {
    fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(256),
            overflowed: false,
        }
    }

    fn push(&mut self, response: &[u8]) {
        if self.overflowed {
            return;
        }
        if response.len() > MAX_PTY_RESPONSE_BYTES.saturating_sub(self.bytes.len()) {
            self.overflowed = true;
            return;
        }
        self.bytes.extend_from_slice(response);
    }
}

#[derive(Debug, Error)]
enum WorkerError {
    #[error("thread error: {0}")]
    Thread(String),
    #[error("spawn error: {0}")]
    Spawn(String),
    #[error("PTY error: {0}")]
    Pty(String),
    #[error("terminal emulation error: {0}")]
    Ghostty(#[from] libghostty_vt::Error),
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("terminal search snapshot exceeds the {MAX_SEARCH_SNAPSHOT_BYTES}-byte limit")]
    SearchSnapshotTooLarge,
    #[error("terminal viewport metadata exceeds its 32-bit in-memory limits")]
    ViewportMetadataTooLarge,
    #[error(
        "terminal reliable-event backlog exceeded {MAX_PENDING_RELIABLE_EVENTS} messages or {MAX_PENDING_RELIABLE_EVENT_BYTES} bytes"
    )]
    EventBackpressure,
    #[error("terminal event consumer stopped")]
    EventConsumerStopped,
}

fn warn_and_skip_view_ghostty<T>(
    context: &str,
    result: Result<T, WorkerError>,
) -> Result<Option<T>, WorkerError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(WorkerError::Ghostty(error)) => {
            log::warn!("skipping terminal {context} after emulation error: {error}");
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn normalize_view_action_result(
    result: Result<ViewActionResult, WorkerError>,
) -> Result<ViewActionResult, WorkerError> {
    match result {
        Err(error @ WorkerError::SearchSnapshotTooLarge) => {
            log::warn!("skipping terminal view action: {error}");
            Ok(ViewActionResult::Snapshot)
        }
        result => result,
    }
}

#[derive(Clone)]
struct Publisher {
    event_tx: TerminalEventSender,
    latest: Arc<RwLock<PublishedViewports>>,
    state: Arc<EventQueueState>,
}

impl Publisher {
    fn set_foreground_source(&self, source: Option<Box<ForegroundSource>>) {
        *self.state.foreground.write() = source;
        self.state.resolve_identity();
    }

    fn set_completion(&self, completion: TerminalProcessExit) {
        self.state
            .completion
            .store(completion.encode(), Ordering::Release);
    }

    fn set_facts(&self, facts: TerminalFacts) {
        let titled = {
            let mut latest = self.latest.write();
            let titled = latest.facts.program_title_writes != facts.program_title_writes;
            latest.facts = facts;
            titled
        };
        if titled {
            self.notify_latest();
        }
    }

    fn set_progress_bar(&self, bar: ProgressBar) {
        let changed = {
            let mut latest = self.latest.write();
            let changed = latest.bar.state != bar.state;
            latest.bar = bar;
            changed
        };
        if changed {
            self.notify_latest();
        }
    }

    fn frame_sink(&self) -> Option<&Arc<dyn TerminalFrameSink>> {
        self.state.sinks.frame.get()
    }

    fn output_sink(&self) -> Option<&Arc<dyn TerminalFrameSink>> {
        self.frame_sink().filter(|sink| sink.takes_output())
    }

    fn takes_output(&self) -> bool {
        self.output_sink().is_some()
    }

    fn output_room(&self) -> usize {
        self.output_sink()
            .map_or(usize::MAX, |sink| sink.output_room())
    }

    fn output(&self, bytes: &Arc<[u8]>) {
        if let Some(sink) = self.output_sink() {
            sink.output(bytes);
        }
    }

    fn set_last_command_status(&self, status: Option<i32>) {
        self.latest.write().last_command_status = status;
    }

    fn push_shell_marks(&self, marks: Vec<ShellMark>) {
        if marks.is_empty() {
            return;
        }
        let mut latest = self.latest.write();
        for mark in marks {
            if latest.shell_marks.len() == MAX_PENDING_SHELL_MARKS {
                latest.shell_marks.pop_front();
                latest.shell_marks_dropped += 1;
            }
            latest.shell_marks.push_back(mark);
        }
    }

    fn set_program_status(&self, status: ProgramStatus) {
        self.latest.write().program_status = Arc::new(status);
        self.notify_latest();
    }

    #[cfg(test)]
    fn publish(&self, viewport: TerminalViewport) {
        let viewport = Arc::new(viewport);
        {
            let mut latest = self.latest.write();
            latest.size = (viewport.columns, viewport.rows);
            latest.fallback = Arc::clone(&viewport);
            latest.fallback_current = true;
            latest.by_view.clear();
            latest.epochs.clear();
            latest.copy_facts.clear();
            latest.sunk.clear();
        }
        self.notify_viewports(&viewport, 0);
    }

    fn publish_frame(
        &self,
        fallback: FallbackFrame,
        viewports: Vec<ViewFrame>,
        copy_facts: HashMap<TerminalViewId, Arc<CopyModeFacts>>,
        sunk: Vec<TerminalViewId>,
        size: (u16, u16),
        notify: bool,
    ) -> bool {
        let mut by_view = HashMap::with_capacity(viewports.len());
        let mut epochs = HashMap::with_capacity(viewports.len());
        let mut first_streamed = None;
        for (view, viewport, epoch) in viewports {
            if let Some(epoch) = epoch {
                first_streamed.get_or_insert_with(|| Arc::clone(&viewport));
                epochs.insert(view, epoch);
            }
            by_view.insert(view, viewport);
        }
        let view_count = by_view.len();
        let (fallback, current) = match fallback {
            FallbackFrame::FirstStreamed => (
                first_streamed.expect("a streamed view published a frame"),
                true,
            ),
            FallbackFrame::Built(viewport) => (Arc::new(viewport), true),
            FallbackFrame::Metadata(viewport) => (Arc::new(viewport), false),
        };
        {
            let mut latest = self.latest.write();
            latest.fallback = Arc::clone(&fallback);
            latest.fallback_current = current;
            latest.by_view = by_view;
            latest.epochs = epochs;
            latest.copy_facts = copy_facts;
            latest.sunk = sunk;
            latest.size = size;
        }
        notify && self.notify_viewports(&fallback, view_count)
    }

    fn refresh_fallback(
        &self,
        terminal: &Terminal<'_, '_>,
        dictionary: &mut ViewportDictionary,
        status: &SessionStatus,
        bound: Option<(u16, u16)>,
    ) -> Result<Option<(TerminalViewport, bool)>, WorkerError> {
        let previous = Arc::clone(&self.latest.read().fallback);
        let scrollbar = terminal.scrollbar()?;
        let scrollbar = ScrollbarState {
            total: u32::try_from(scrollbar.total)
                .map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
            offset: u32::try_from(scrollbar.offset)
                .map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
            len: u32::try_from(scrollbar.len).map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
        };
        let at_bottom = |state: ScrollbarState| {
            u64::from(state.offset) + u64::from(state.len) >= u64::from(state.total)
        };
        if (previous.columns, previous.rows)
            != bounded_extent((terminal.cols()?, terminal.rows()?), bound)
            || previous.mode != TerminalMode::Live
            || !previous.overlays.is_empty()
            || previous.search.is_some()
            || !at_bottom(previous.scrollbar)
            || !at_bottom(scrollbar)
        {
            return Ok(None);
        }
        let title = terminal.title().unwrap_or("zz");
        let working_directory = terminal.pwd().ok().and_then(reported_working_directory);
        let mut viewport = (*previous).clone();
        viewport.presentation =
            dictionary.shared_presentation(title, working_directory.as_deref(), None);
        viewport.status = status.clone();
        viewport.scrollbar = scrollbar;
        viewport.mouse_tracking = terminal.is_mouse_tracking()?;
        viewport.kitty_keyboard = terminal
            .kitty_keyboard_flags()
            .is_ok_and(|flags| !flags.is_empty());
        viewport.unseen_output = 0;
        let changed = viewport.presentation.title != previous.presentation.title
            || viewport.presentation.working_directory != previous.presentation.working_directory
            || viewport.status != previous.status
            || viewport.mouse_tracking != previous.mouse_tracking
            || viewport.kitty_keyboard != previous.kitty_keyboard;
        Ok(Some((viewport, changed)))
    }

    fn latest_fallback(&self) -> Arc<TerminalViewport> {
        Arc::clone(&self.latest.read().fallback)
    }

    fn notify_latest(&self) {
        let fallback = self.latest_fallback();
        self.notify_viewports(&fallback, 0);
    }

    fn publish_search_string(&self, search: Option<&CopyModeSearch>) {
        let text = search.map_or("", |search| search.text.as_str());
        if self.latest.read().search_string != text {
            text.clone_into(&mut self.latest.write().search_string);
        }
    }

    fn notify_viewports(&self, viewport: &TerminalViewport, view_count: usize) -> bool {
        let notification_was_pending = self.state.notification_pending.swap(true, Ordering::AcqRel);
        let notified = notification_was_pending
            || match self.event_tx.try_send(TerminalEvent::ViewportReady {
                output_activity: false,
            }) {
                Ok(()) => {
                    self.state.notify_consumer();
                    true
                }
                Err(async_channel::TrySendError::Closed(_)) => false,
                Err(async_channel::TrySendError::Full(_)) => {
                    self.state
                        .notification_pending
                        .store(false, Ordering::Release);
                    log::error!("terminal viewport notification queue overflow");
                    false
                }
            };
        if log::log_enabled!(
            target: "zz_terminal::diagnostics::publisher",
            log::Level::Trace
        ) {
            log::trace!(
                target: "zz_terminal::diagnostics::publisher",
                "publish generation={} view_generation={} columns={} rows={} cells={} overlays={} status={:?} views={view_count} notification_was_pending={notification_was_pending} event_queue_len={} event_queue_capacity={:?} pending_reliable={} pending_reliable_bytes={}",
                viewport.generation,
                viewport.view_generation,
                viewport.columns,
                viewport.rows,
                viewport.cells.len(),
                viewport.overlays.len(),
                viewport.status,
                self.event_tx.len(),
                self.event_tx.capacity(),
                self.state.pending_reliable.load(Ordering::Acquire),
                self.state.pending_reliable_bytes.load(Ordering::Acquire),
            );
        }
        notified
    }

    fn set_status(&self, status: &SessionStatus) {
        let update = |viewport: &TerminalViewport| {
            let mut viewport = viewport.clone();
            viewport.generation = viewport.generation.saturating_add(1);
            viewport.view_generation = viewport.view_generation.saturating_add(1);
            viewport.status = status.clone();
            Arc::new(viewport)
        };
        let (fallback, view_count) = {
            let mut latest = self.latest.write();
            latest.fallback = update(&latest.fallback);
            let by_view = std::mem::take(&mut latest.by_view)
                .into_iter()
                .map(|(view, viewport)| (view, update(&viewport)))
                .collect::<HashMap<_, _>>();
            let view_count = by_view.len();
            latest.by_view = by_view;
            latest.sunk.clear();
            (Arc::clone(&latest.fallback), view_count)
        };
        self.notify_viewports(&fallback, view_count);
    }

    fn mark_output_activity(&self) {
        self.state
            .output_activity_pending
            .store(true, Ordering::Release);
        self.state.mark_output();
    }

    fn fail(&self, error: &WorkerError) {
        self.set_status(&SessionStatus::failed(error.to_string()));
        self.state.resolve_identity();
    }

    fn send_reliable(&self, event: TerminalEvent) -> Result<(), WorkerError> {
        let bytes = reliable_event_bytes(&event);
        self.state
            .pending_reliable
            .try_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                (pending < MAX_PENDING_RELIABLE_EVENTS).then_some(pending + 1)
            })
            .map_err(|_| WorkerError::EventBackpressure)?;
        if self
            .state
            .pending_reliable_bytes
            .try_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                (bytes <= MAX_PENDING_RELIABLE_EVENT_BYTES.saturating_sub(pending))
                    .then_some(pending + bytes)
            })
            .is_err()
        {
            self.state.pending_reliable.fetch_sub(1, Ordering::AcqRel);
            return Err(WorkerError::EventBackpressure);
        }
        match self.event_tx.try_send(event) {
            Ok(()) => {
                self.state.notify_consumer();
                Ok(())
            }
            Err(async_channel::TrySendError::Full(_)) => {
                self.release_reliable(bytes);
                Err(WorkerError::EventBackpressure)
            }
            Err(async_channel::TrySendError::Closed(_)) => {
                self.release_reliable(bytes);
                Err(WorkerError::EventConsumerStopped)
            }
        }
    }

    fn release_reliable(&self, bytes: usize) {
        let previous = self.state.pending_reliable.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "reliable terminal event accounting underflow");
        let previous_bytes = self
            .state
            .pending_reliable_bytes
            .fetch_sub(bytes, Ordering::AcqRel);
        debug_assert!(
            previous_bytes >= bytes,
            "reliable terminal event byte accounting underflow"
        );
    }

    fn copy_ready(
        &self,
        view: TerminalViewId,
        copy: Box<TerminalCopyReady>,
    ) -> Result<(), WorkerError> {
        self.send_user_action(TerminalEvent::CopyReady { view, copy }, "copy result")
    }

    fn open_uri(&self, view: TerminalViewId, uri: String) -> Result<(), WorkerError> {
        self.send_user_action(
            TerminalEvent::OpenUri(Box::new(TerminalOpenUri { view, uri })),
            "open-URI request",
        )
    }

    fn view_closed(&self, view: TerminalViewId) -> Result<(), WorkerError> {
        self.send_reliable(TerminalEvent::ViewClosed(view))
    }

    fn clipboard_set(&self, target: ClipboardTarget, text: String) -> Result<(), WorkerError> {
        self.send_user_action(
            TerminalEvent::ClipboardSet { target, text },
            "clipboard write",
        )
    }

    fn rename_window(&self, name: String) -> Result<(), WorkerError> {
        self.send_user_action(TerminalEvent::RenameWindow(name), "window rename")
    }

    fn bell(&self) {
        if let Err(error) = self.send_reliable(TerminalEvent::Bell)
            && matches!(error, WorkerError::EventBackpressure)
        {
            log::warn!("discarding terminal bell: reliable event backlog is full");
        }
    }

    fn placeholder_bound(&self, token: u64, number: u32) -> Result<(), WorkerError> {
        self.send_reliable(TerminalEvent::PlaceholderBound { token, number })
    }

    fn pending_paste_expired(&self, token: u64) -> Result<(), WorkerError> {
        self.send_reliable(TerminalEvent::PendingPasteExpired { token })
    }

    fn send_user_action(&self, event: TerminalEvent, description: &str) -> Result<(), WorkerError> {
        match self.send_reliable(event) {
            Err(WorkerError::EventBackpressure) => {
                log::warn!("discarding terminal {description}: reliable event backlog is full");
                Ok(())
            }
            result => result,
        }
    }
}

fn reliable_event_bytes(event: &TerminalEvent) -> usize {
    let payload = match event {
        TerminalEvent::CopyReady { copy, .. } => copy
            .text
            .len()
            .saturating_add(copy.pipe.as_ref().map_or(0, String::len))
            .saturating_add(copy.buffer.as_ref().map_or(0, |buffer| match buffer {
                PasteBufferAction::Create { prefix } => prefix.as_ref().map_or(0, String::len),
                PasteBufferAction::Append => 0,
            })),
        TerminalEvent::OpenUri(open) => open.uri.len(),
        TerminalEvent::ClipboardSet { text, .. } => text.len(),
        TerminalEvent::RenameWindow(name) => name.len(),
        TerminalEvent::ViewportReady { .. }
        | TerminalEvent::ViewClosed(_)
        | TerminalEvent::Bell
        | TerminalEvent::PlaceholderBound { .. }
        | TerminalEvent::PendingPasteExpired { .. } => 0,
    };
    std::mem::size_of::<TerminalEvent>().saturating_add(payload)
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the worker thread must own its channel and publisher"
)]
fn terminal_worker(
    control_rx: Receiver<Command>,
    input_rx: InputReceiver,
    slot: Arc<Mutex<ControlSlot>>,
    publisher: Publisher,
    max_scrollback: usize,
    appearance: Arc<TerminalAppearance>,
    spawn: TerminalSpawn,
    wake: ActorWake,
    wake_rx: WakeReceiver,
) {
    if let Err(error) = run_terminal(
        control_rx,
        input_rx,
        slot,
        publisher.clone(),
        max_scrollback,
        &appearance,
        &spawn,
        &wake,
        wake_rx,
    ) {
        report_worker_error(&publisher, &error);
    }
    publisher.set_foreground_source(None);
}

fn report_worker_error(publisher: &Publisher, error: &WorkerError) {
    log::error!("terminal worker stopped: {error}");
    if matches!(error, WorkerError::Spawn(_)) {
        publisher.set_completion(TerminalProcessExit {
            code: 1,
            signal: None,
        });
        publisher.set_status(&SessionStatus::exited(1, None));
    } else {
        publisher.fail(error);
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the worker thread owns its command-output source and channels"
)]
fn output_view_worker(
    command_rx: Receiver<Command>,
    slot: Arc<Mutex<ControlSlot>>,
    publisher: Publisher,
    title: String,
    text: String,
    appearance: Arc<TerminalAppearance>,
    max_scrollback: usize,
    frozen: bool,
    geometry: Geometry,
) {
    let result = (|| {
        let mut actor = new_output_view(
            command_rx,
            slot,
            publisher.clone(),
            &title,
            &text,
            &appearance,
            max_scrollback,
            frozen,
            geometry,
            &ActorWake::none(),
        )?;
        loop {
            actor.on_deadline()?;
            let wake = actor.wait_for_wake()?;
            if !actor.on_wake(wake)? {
                return Ok::<(), WorkerError>(());
            }
        }
    })();
    if let Err(error) = result {
        log::error!("command output view stopped: {error}");
        publisher.fail(&error);
    }
}

fn apply_terminal_appearance(
    terminal: &mut Terminal<'_, '_>,
    appearance: &TerminalAppearance,
) -> Result<(), WorkerError> {
    let follows_default = [
        terminal.fg_color()? == terminal.default_fg_color()?,
        terminal.bg_color()? == terminal.default_bg_color()?,
    ];
    terminal.set_default_fg_color(Some(ghostty_color(appearance.foreground)))?;
    terminal.set_default_bg_color(Some(ghostty_color(appearance.background)))?;
    if follows_default[0] && terminal.fg_color()? != terminal.default_fg_color()? {
        terminal.vt_write(b"\x1b]110\x1b\\");
    }
    if follows_default[1] && terminal.bg_color()? != terminal.default_bg_color()? {
        terminal.vt_write(b"\x1b]111\x1b\\");
    }
    terminal.set_default_cursor_color(Some(ghostty_color(appearance.cursor_color)))?;
    terminal.set_default_cursor_style(Some(ghostty_cursor_style(appearance.cursor_style)))?;
    terminal.set_default_cursor_blink(Some(!matches!(
        appearance.cursor_blink_policy,
        CursorBlinkPolicy::Off
    )))?;
    terminal.set_default_color_palette(Some(Palette(
        (*appearance.palette.as_array()).map(ghostty_color),
    )))?;
    log::debug!(
        target: "zz_terminal::diagnostics::appearance",
        "applied appearance hash={} foreground={:?} background={:?} cursor={:?} cursor_style={:?} palette_entries=256",
        appearance.stable_hash(),
        appearance.foreground,
        appearance.background,
        appearance.cursor_color,
        appearance.cursor_style,
    );
    Ok(())
}

const fn ghostty_color(value: Color) -> RgbColor {
    RgbColor {
        r: value.r,
        g: value.g,
        b: value.b,
    }
}

const fn ghostty_cursor_style(value: CursorStyle) -> GhosttyCursorStyle {
    match value {
        CursorStyle::Bar => GhosttyCursorStyle::Bar,
        CursorStyle::Block => GhosttyCursorStyle::Block,
        CursorStyle::Underline => GhosttyCursorStyle::Underline,
        CursorStyle::BlockHollow => GhosttyCursorStyle::BlockHollow,
    }
}

const fn ghostty_color_scheme(value: TerminalColorScheme) -> ColorScheme {
    match value {
        TerminalColorScheme::Light => ColorScheme::Light,
        TerminalColorScheme::Dark => ColorScheme::Dark,
    }
}

fn new_terminal<'alloc: 'cb, 'cb>(
    cols: u16,
    rows: u16,
    history_limit: usize,
) -> Result<Terminal<'alloc, 'cb>, libghostty_vt::Error> {
    let history_limit = history_limit.min(MAX_HISTORY_LIMIT);
    let mut terminal = Terminal::new(cols, rows)?;
    terminal
        .set_scrollback_max_bytes(Some(scrollback_backstop_bytes(history_limit, cols)))?
        .set_scrollback_max_lines(Some(history_limit))?;
    Ok(terminal)
}

const SCROLLBACK_BACKSTOP_BYTES_PER_CELL: usize = 128;
const SCROLLBACK_BACKSTOP_FLOOR: usize = 64 * 1024 * 1024;
const SCROLLBACK_BACKSTOP_CAP: usize = 1024 * 1024 * 1024;

fn scrollback_backstop_bytes(history_limit: usize, columns: u16) -> usize {
    history_limit
        .saturating_mul(usize::from(columns.max(1)))
        .saturating_mul(SCROLLBACK_BACKSTOP_BYTES_PER_CELL)
        .clamp(SCROLLBACK_BACKSTOP_FLOOR, SCROLLBACK_BACKSTOP_CAP)
}

fn clipboard_write_request<'a>(
    location: ClipboardLocation,
    contents: impl Iterator<Item = ClipboardContent<'a>>,
) -> Result<(ClipboardTarget, String), ClipboardWriteError> {
    let mut selected = None;
    for content in contents {
        if content.mime == CLIPBOARD_TEXT_MIME {
            selected = Some(content);
            break;
        }
        selected.get_or_insert(content);
    }
    let content = selected.ok_or(ClipboardWriteError::Unsupported)?;
    if content.data.len() > MAX_CLIPBOARD_WRITE_BYTES {
        return Err(ClipboardWriteError::Denied);
    }
    let target = match location {
        ClipboardLocation::Standard => ClipboardTarget::Clipboard,
        ClipboardLocation::Selection | ClipboardLocation::Primary => ClipboardTarget::Primary,
    };
    Ok((target, String::from_utf8_lossy(content.data).into_owned()))
}

fn register_clipboard_write(
    terminal: &mut Terminal<'_, '_>,
    publisher: Publisher,
) -> Result<(), WorkerError> {
    terminal.on_clipboard_write(move |_, write| {
        let result = clipboard_write_request(write.location(), write.contents()).and_then(
            |(target, text)| {
                publisher
                    .clipboard_set(target, text)
                    .map_err(|_| ClipboardWriteError::Busy)
            },
        );
        write.reply(result, false);
    })?;
    Ok(())
}

fn register_bell(terminal: &mut Terminal<'_, '_>, publisher: Publisher) -> Result<(), WorkerError> {
    terminal.on_bell(move |_| publisher.bell())?;
    Ok(())
}

fn register_device_attributes(terminal: &mut Terminal<'_, '_>) -> Result<(), WorkerError> {
    terminal.on_device_attributes(|_| {
        Some(DeviceAttributes {
            primary: PrimaryDeviceAttributes::new(
                ConformanceLevel::VT220,
                &[DeviceAttributeFeature::ANSI_COLOR],
            ),
            secondary: SecondaryDeviceAttributes {
                device_type: DeviceType::VT220,
                firmware_version: 1,
                rom_cartridge: 0,
            },
            tertiary: TertiaryDeviceAttributes { unit_id: 0 },
        })
    })?;
    Ok(())
}

fn new_output_view(
    command_rx: Receiver<Command>,
    slot: Arc<Mutex<ControlSlot>>,
    publisher: Publisher,
    title: &str,
    text: &str,
    appearance: &TerminalAppearance,
    max_scrollback: usize,
    frozen: bool,
    geometry: Geometry,
    wake: &ActorWake,
) -> Result<surface_actor::SurfaceActor<'static, 'static>, WorkerError> {
    install_kitty_png_decoder();
    let mut terminal = new_terminal(geometry.columns, geometry.rows, max_scrollback)?;
    let reported_color_scheme = Rc::new(Cell::new(ghostty_color_scheme(appearance.color_scheme)));
    let color_scheme_source = Rc::clone(&reported_color_scheme);
    terminal.on_color_scheme(move |_| Some(color_scheme_source.get()))?;
    apply_terminal_appearance(&mut terminal, appearance)?;
    register_bell(&mut terminal, publisher.clone())?;
    if frozen {
        write_output_view_content(&mut terminal, title, text);
    } else {
        terminal.vt_write(text.as_bytes());
    }
    let mut actor = surface_actor::SurfaceActor::new(
        command_rx,
        slot,
        publisher,
        SurfaceTerminal {
            terminal,
            geometry,
            frames: Frames::new(appearance)?,
            active_views: ActiveTerminalViews::new(),
            inactive_views: InactiveTerminalViews::new(),
            word_separators: WordSeparators::default(),
            wrap_search: true,
            mode_keys_vi: false,
            reported_color_scheme,
            max_scrollback,
            status: SessionStatus::Running,
            pending_commands: Vec::new(),
            captures: VecDeque::new(),
            pending_copy_source: None,
            pane_search: None,
            search: Some(SearchWorker::spawn(wake.clone())),
        },
        EngineFilter::default(),
        frozen,
    )?;
    if !frozen {
        actor.publish_started()?;
    }
    Ok(actor)
}

struct SurfaceTerminal<'a, 'b> {
    terminal: Terminal<'a, 'b>,
    geometry: Geometry,
    frames: Frames<'a>,
    active_views: ActiveTerminalViews,
    inactive_views: InactiveTerminalViews,
    word_separators: WordSeparators,
    wrap_search: bool,
    mode_keys_vi: bool,
    reported_color_scheme: Rc<Cell<ColorScheme>>,
    max_scrollback: usize,
    status: SessionStatus,
    pending_commands: Vec<Command>,
    captures: VecDeque<CaptureWork>,
    pending_copy_source: Option<Box<CapturedCopySource>>,
    pane_search: Option<CopyModeSearch>,
    search: Option<(SearchWorker, Receiver<SearchResults>)>,
}

fn write_output_view_content(terminal: &mut Terminal<'_, '_>, title: &str, text: &str) {
    let title = title
        .chars()
        .filter(|character| !character.is_control())
        .take(256)
        .collect::<String>();
    terminal.vt_write(format!("\x1b]2;{title}\x07").as_bytes());
    for (index, line) in text.split('\n').enumerate() {
        if index != 0 {
            terminal.vt_write(b"\r\n");
        }
        terminal.vt_write(line.strip_suffix('\r').unwrap_or(line).as_bytes());
    }
}

fn output_view_state(terminal: &mut Terminal<'_, '_>) -> Result<TerminalViewState, WorkerError> {
    let mut view = TerminalViewState::for_screen(terminal.active_screen()?);
    let active = view.active_mut();
    enter_copy_mode(
        terminal,
        &mut active.selection,
        &mut active.copy_mode,
        false,
        false,
        None,
        false,
    )?;
    let mode = active
        .copy_mode
        .as_mut()
        .expect("entering output view creates a frozen revision");
    mode.kind = FrozenModeKind::View;
    mode.line_numbers = 0;
    mode.cursor = PointCoordinate { x: 0, y: 0 };
    mode.viewport_offset = 0;
    Ok(view)
}

fn refresh_output_view(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<(), WorkerError> {
    let Some(mode) = view.active_mut().copy_mode.as_mut() else {
        return Ok(());
    };
    let old_total = mode.revision.total_rows();
    let old_maximum = mode.revision.maximum_offset();
    let old_cursor = mode.cursor;
    let old_offset = mode.viewport_offset;
    let revision = ModeRevision::capture(terminal)?;
    let scale = |value: u32, old_limit: u32, new_limit: u32| {
        if old_limit == 0 {
            0
        } else {
            u32::try_from(
                u64::from(value)
                    .saturating_mul(u64::from(new_limit))
                    .saturating_div(u64::from(old_limit)),
            )
            .unwrap_or(new_limit)
            .min(new_limit)
        }
    };
    mode.cursor = revision.clamp_point(PointCoordinate {
        x: old_cursor.x.min(revision.columns.saturating_sub(1)),
        y: scale(
            old_cursor.y,
            old_total.saturating_sub(1),
            revision.total_rows().saturating_sub(1),
        ),
    });
    mode.viewport_offset = scale(old_offset, old_maximum, revision.maximum_offset());
    mode.revision = revision;
    mode.selection = None;
    mode.mark = None;
    mode.last_jump = None;
    mode.selecting = false;
    view.search_snapshot = None;
    if view.search.is_some() {
        complete_view_search(terminal, view)?;
    }
    Ok(())
}

fn resize_copy_modes(
    views: &mut ActiveTerminalViews,
    columns: u16,
    rows: u16,
    worker: &mut SearchWorker,
) -> Result<(), WorkerError> {
    for (view_id, view) in views {
        let request_id = worker.cancel(*view_id);
        for state in [&mut view.primary, &mut view.alternate] {
            let Some(mode) = state.copy_mode.as_mut() else {
                continue;
            };
            let (revision, cursor) = mode.revision.resized(columns, rows, mode.cursor)?;
            mode.revision = revision;
            mode.cursor = mode.revision.clamp_point(cursor);
            mode.viewport_offset = mode.cursor.y.min(mode.revision.maximum_offset());
            mode.selection = None;
            mode.selecting = false;
            mode.recentre = None;
            rebuild_copy_search(
                mode,
                &mut state.search,
                &mut state.search_snapshot,
                request_id,
            );
        }
    }
    Ok(())
}

fn rebuild_copy_search(
    mode: &mut CopyModeState,
    search: &mut SearchSlot,
    search_snapshot: &mut Option<Arc<HistorySearchSnapshot>>,
    request_id: u64,
) {
    if let Some(origin) = mode.incremental_origin.as_mut() {
        origin.row = mode.cursor.y;
        origin.viewport_offset = mode.viewport_offset;
    }
    *search_snapshot = None;
    if mode.search_marks
        && let Some(previous) = search.take()
    {
        let mut rebuilt = mode
            .revision
            .search
            .search(&previous.query, request_id, || false)
            .expect("copy search rebuild");
        rebuilt.current = rebuilt
            .matches
            .iter()
            .position(|found| found.contains(mode.cursor, mode.revision.columns));
        mode.search_count = Some((
            u32::try_from(rebuilt.matches.len()).unwrap_or(u32::MAX),
            false,
        ));
        *search_snapshot = Some(Arc::clone(&mode.revision.search));
        *search = Some(Box::new(rebuilt));
    } else {
        mode.search_marks = false;
        mode.search_count = None;
        *search = None;
    }
}

fn settle_copy_search(
    view_id: TerminalViewId,
    worker: &mut SearchWorker,
    copy_mode: &mut CopyModeSlot,
    search: &mut SearchSlot,
    search_origin: &mut Option<PointCoordinate>,
    search_snapshot: &mut Option<Arc<HistorySearchSnapshot>>,
    revision_before: Option<u64>,
) {
    let Some(before) = revision_before else {
        return;
    };
    match copy_mode.as_deref_mut() {
        None => drop_view_search(view_id, worker, search, search_origin, search_snapshot),
        Some(mode) if mode.revision.id != before => {
            let request_id = worker.cancel(view_id);
            rebuild_copy_search(mode, search, search_snapshot, request_id);
        }
        Some(_) => {}
    }
}

fn refresh_frozen_view_appearance(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<bool, WorkerError> {
    let Some(mode) = view.active_mut().copy_mode.as_mut() else {
        return Ok(false);
    };
    if mode.revision.matches_terminal_appearance(terminal)? {
        return Ok(false);
    }

    let revision = mode.revision.with_appearance(terminal)?;
    mode.cursor = revision.clamp_point(mode.cursor);
    mode.viewport_offset = mode.viewport_offset.min(revision.maximum_offset());
    if let Some(selection) = mode.selection.as_mut() {
        selection.anchor = revision.clamp_point(selection.anchor);
        selection.focus = revision.clamp_point(selection.focus);
    }
    if let Some(mark) = mode.mark.as_mut() {
        *mark = revision.clamp_point(*mark);
    }
    mode.revision = revision;
    view.search_snapshot = None;
    if view.search.is_some() {
        complete_view_search(terminal, view)?;
    }
    Ok(true)
}

fn terminal_command(spawn: &TerminalSpawn) -> CommandBuilder {
    match spawn.command.as_deref() {
        Some(argv) if argv.len() >= 2 => {
            let mut command = CommandBuilder::new(&argv[0]);
            command.args(&argv[1..]);
            command
        }
        #[cfg(windows)]
        Some([shell_command]) => {
            let mut command = CommandBuilder::new("cmd.exe");
            let shell = command.get_shell();
            command.args([
                "/D",
                "/V:ON",
                "/C",
                shell.as_str(),
                "/D",
                "/S",
                "/C",
                "!ZZ_PANE_COMMAND!",
            ]);
            command.env(
                "ZZ_PANE_COMMAND",
                format!("\"set ZZ_PANE_COMMAND=& {shell_command}\""),
            );
            command
        }
        #[cfg(not(windows))]
        Some([shell_command]) => {
            let mut command = CommandBuilder::new(spawn.shell.as_deref().unwrap_or("/bin/sh"));
            command.args(["-c", shell_command]);
            command
        }
        None if spawn.non_login_shell => {
            CommandBuilder::new(spawn.shell.as_deref().unwrap_or("/bin/sh"))
        }
        _ => crate::shell_integration::default_shell_command(spawn.shell.as_deref()),
    }
}

fn run_terminal(
    control_rx: Receiver<Command>,
    input_rx: InputReceiver,
    slot: Arc<Mutex<ControlSlot>>,
    publisher: Publisher,
    max_scrollback: usize,
    appearance: &TerminalAppearance,
    spawn: &TerminalSpawn,
    wake: &ActorWake,
    wake_rx: WakeReceiver,
) -> Result<(), WorkerError> {
    let actor = PaneActor::spawn(
        control_rx,
        input_rx,
        slot,
        publisher,
        max_scrollback,
        appearance,
        spawn,
        wake,
        wake_rx,
        false,
        #[cfg(target_os = "linux")]
        pty_gather_enabled()
            .then(|| PtyGather::start("zz-pty-gather".to_owned()))
            .transpose()?,
    )?;
    let mut actor = shard::Actor::Live(Box::new(actor));
    loop {
        let Some(next) = actor.on_deadline()? else {
            return Ok(());
        };
        actor = next;
        actor.end_turn();
        let wake = actor.wait_for_wake()?;
        actor.begin_turn();
        let Some(next) = actor.on_wake(wake)? else {
            return Ok(());
        };
        actor = next;
    }
}

fn reply_before_dead_notice(
    terminal: &mut Terminal<'_, '_>,
    class_hints: &ClassHints,
    active_views: &ActiveTerminalViews,
    word_separators: &WordSeparators,
    slot: &Mutex<ControlSlot>,
    command: Command,
) -> Option<Command> {
    match command {
        Command::Capture(request) => {
            let CaptureRequest { options, reply } = *request;
            let mut copy_modes = active_views
                .values()
                .filter_map(|view| view.copy_mode.as_deref());
            let mode = match (copy_modes.next(), copy_modes.next()) {
                (Some(mode), None) => Some(mode),
                _ => None,
            };
            let _ = reply.send(capture_terminal(terminal, mode, options));
        }
        Command::History(request) => {
            let HistoryCommand {
                start,
                count,
                reply,
            } = *request;
            let _ = reply.send(capture_history(terminal, start, count, class_hints, None));
        }
        Command::CaptureCopySource { reply } => {
            let _ = reply.send(
                capture_copy_source(terminal).map_err(|_| TerminalCaptureError::ActorStopped),
            );
        }
        Command::SemanticCapture(request) => {
            let _ = request.reply.send(capture_last_command(terminal));
        }
        Command::PointerContext(request) => {
            let PointerContextRequest {
                view,
                column,
                row,
                reply,
            } = *request;
            let mode = active_views
                .get(&view)
                .and_then(|view| view.copy_mode.as_deref());
            let _ = reply.send(
                pointer_context(terminal, mode, column, row, word_separators).unwrap_or_default(),
            );
        }
        command => return Some(command),
    }
    complete_dead_notice_command(slot);
    None
}

fn complete_dead_notice_command(slot: &Mutex<ControlSlot>) {
    let mut slot = slot.lock();
    slot.in_flight = slot.in_flight.saturating_sub(1);
    for (remaining, _) in &mut slot.deferred {
        *remaining = remaining.saturating_sub(1);
    }
}

fn write_dead_notice(terminal: &mut Terminal<'_, '_>, text: &str) -> Result<(), WorkerError> {
    if text.is_empty() {
        terminal.vt_write(b"\x1b[?25l");
        return Ok(());
    }
    let columns = usize::from(terminal.cols()?);
    let rows = terminal.rows()?;
    let mut drawn = String::with_capacity(text.len());
    let mut width = 0_usize;
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '\x1b' {
            drawn.push(character);
            for control in characters.by_ref() {
                drawn.push(control);
                if control == 'm' {
                    break;
                }
            }
            continue;
        }
        let advance = usize::from(libghostty_vt::unicode::codepoint_width(character));
        if width + advance > columns {
            break;
        }
        width += advance;
        drawn.push(character);
    }
    terminal.vt_write(format!("\x1b[r\x1b[{rows};1H\n").as_bytes());
    terminal.vt_write(drawn.as_bytes());
    terminal.vt_write(b"\x1b[0m\x1b[?25l");
    Ok(())
}

#[cfg(unix)]
fn signal_number(description: &str) -> Option<u8> {
    if let Some(number) = description
        .strip_prefix("Signal ")
        .and_then(|number| number.parse().ok())
        .or_else(|| {
            description
                .rsplit_once(':')
                .and_then(|(_, number)| number.trim().parse().ok())
        })
    {
        return Some(number);
    }
    match description {
        "Hangup" => Some(1),
        "Interrupt" => Some(2),
        "Quit" => Some(3),
        "Illegal instruction" => Some(4),
        "Trace/breakpoint trap" | "Trace trap" => Some(5),
        "Aborted" | "Abort trap" => Some(6),
        "Bus error" => Some(7),
        "Floating point exception" => Some(8),
        "Killed" => Some(9),
        "Segmentation fault" => Some(11),
        "Broken pipe" => Some(13),
        "Alarm clock" => Some(14),
        "Terminated" => Some(15),
        _ => None,
    }
}

#[cfg(not(unix))]
fn signal_number(_description: &str) -> Option<u8> {
    None
}

#[cfg(unix)]
fn signal_terminal_process_groups(
    master: &unix_pty::UnixMaster,
    shell_process_id: Option<u32>,
    signal: rustix::process::Signal,
) {
    let shell = shell_process_id
        .filter(|group| *group != 0)
        .and_then(|group| i32::try_from(group).ok())
        .and_then(rustix::process::Pid::from_raw);
    let foreground = master
        .process_group_leader()
        .and_then(rustix::process::Pid::from_raw)
        .filter(|group| Some(*group) != shell);
    if let Some(group) = foreground {
        let _ = rustix::process::kill_process_group(group, signal);
    }
    if let Some(shell) = shell
        && rustix::process::kill_process_group(shell, signal) == Err(rustix::io::Errno::SRCH)
    {
        let _ = rustix::process::kill_process(shell, signal);
    }
}

enum ViewActionResult {
    None,
    Snapshot,
    OverlaySnapshot,
    ContentSnapshot,
    Copy(Box<TerminalCopyReady>),
    OpenUri(String),
}

fn sync_viewport_anchor(
    terminal: &Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<(), WorkerError> {
    if view.copy_mode.is_some() {
        return Ok(());
    }
    let scrollbar = terminal.scrollbar()?;
    if scrollbar.offset.saturating_add(scrollbar.len) >= scrollbar.total {
        view.viewport = ViewportAnchor::FollowBottom;
        view.unseen_output = 0;
    } else {
        view.viewport = ViewportAnchor::Pinned(
            terminal.track_grid_ref(Point::Viewport(PointCoordinate { x: 0, y: 0 }))?,
        );
    }
    Ok(())
}

fn scroll_to_offset(
    terminal: &mut Terminal<'_, '_>,
    copy_mode: &mut CopyModeSlot,
    unseen_output: &mut u32,
    target: u32,
) -> Result<(), WorkerError> {
    if let Some(mode) = copy_mode.as_mut() {
        mode.viewport_offset = target.min(mode.revision.maximum_offset());
        return Ok(());
    }

    let scrollbar = terminal.scrollbar()?;
    let target = u64::from(target).min(scrollbar.total.saturating_sub(scrollbar.len));
    let delta = i128::from(target).saturating_sub(i128::from(scrollbar.offset));
    terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize_i128(delta)));
    let landed = terminal.scrollbar()?;
    if landed.offset.saturating_add(landed.len) >= landed.total {
        *unseen_output = 0;
    }
    Ok(())
}

fn install_view_selection(
    terminal: &Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let result = install_view_selection_inner(terminal, view, word_separators);
    finish_view_selection_install(terminal, view, result)
}

fn finish_view_selection_install(
    terminal: &Terminal<'_, '_>,
    view: &mut TerminalViewState,
    result: Result<(), WorkerError>,
) -> Result<(), WorkerError> {
    if warn_and_skip_view_ghostty("selection reinstall", result)?.is_none() {
        view.selection = None;
        if let Err(error) = terminal.set_selection(None) {
            log::warn!("failed to clear invalid terminal selection: {error}");
        }
    }
    Ok(())
}

fn install_view_selection_inner(
    terminal: &Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let Some(state) = view.selection.as_ref() else {
        terminal.set_selection(None)?;
        return Ok(());
    };
    let Some(anchor) = state.anchor.snapshot(terminal)? else {
        view.selection = None;
        terminal.set_selection(None)?;
        return Ok(());
    };
    let Some(focus) = state.focus.snapshot(terminal)? else {
        view.selection = None;
        terminal.set_selection(None)?;
        return Ok(());
    };
    let cell_selection_has_extent = if state.mode == SelectionMode::Cell {
        terminal
            .point_from_grid_ref(&anchor, PointSpace::Screen)?
            .zip(terminal.point_from_grid_ref(&focus, PointSpace::Screen)?)
            .is_some_and(|(anchor, focus)| anchor != focus)
    } else {
        true
    };
    let selection = match state.mode {
        SelectionMode::Cell => {
            cell_selection_has_extent.then(|| Selection::new(anchor, focus, state.rectangle))
        }
        SelectionMode::Word => {
            let start = select_native_word_between(terminal, &anchor, &focus, word_separators)?;
            let end = select_native_word_between(terminal, &focus, &anchor, word_separators)?;
            start
                .zip(end)
                .map(|(start, end)| Selection::new(start.start(), end.end(), false))
        }
        SelectionMode::Line => {
            let start = terminal.select_line(SelectLineOptions::new(anchor))?;
            let end = terminal.select_line(SelectLineOptions::new(focus))?;
            start
                .zip(end)
                .map(|(start, end)| Selection::new(start.start(), end.end(), false))
        }
    };
    terminal.set_selection(selection.as_ref())?;
    Ok(())
}

fn restore_view_state(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    if warn_and_skip_view_ghostty(
        "view-state restore",
        restore_view_state_inner(terminal, view, word_separators),
    )?
    .is_none()
    {
        view.selection = None;
        view.viewport = ViewportAnchor::FollowBottom;
        terminal.scroll_viewport(ScrollViewport::Bottom);
        if let Err(error) = terminal.set_selection(None) {
            log::warn!("failed to clear selection after view-state restore error: {error}");
        }
    }
    Ok(())
}

fn restore_view_state_inner(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    if view.copy_mode.is_some() {
        terminal.set_selection(None)?;
        return Ok(());
    }
    match &view.viewport {
        ViewportAnchor::FollowBottom => terminal.scroll_viewport(ScrollViewport::Bottom),
        ViewportAnchor::Pinned(anchor) => {
            if let Some(point) = anchor.point(PointSpace::Screen)? {
                terminal.scroll_viewport(ScrollViewport::Top);
                terminal
                    .scroll_viewport(ScrollViewport::Delta(saturating_isize(i64::from(point.y))));
            } else {
                view.viewport = ViewportAnchor::FollowBottom;
                terminal.scroll_viewport(ScrollViewport::Bottom);
            }
        }
    }
    install_view_selection(terminal, view, word_separators)
}

fn reconcile_view_screen(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<bool, WorkerError> {
    Ok(warn_and_skip_view_ghostty(
        "view-screen reconciliation",
        reconcile_view_screen_inner(terminal, view, word_separators),
    )?
    .unwrap_or(false))
}

fn note_output_and_revalidate_image_hover(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    output_screen: Screen,
    word_separators: &WordSeparators,
    bound_pasted_images: &HashSet<u32>,
) -> Result<(), WorkerError> {
    let previous_screen = view.screen;
    let previous_hover = view
        .active()
        .hover_link
        .as_ref()
        .filter(|link| pasted_image_number(&link.uri).is_some())
        .cloned();
    view.note_output(output_screen);
    reconcile_view_screen(terminal, view, word_separators)?;
    let Some(previous_hover) = previous_hover.filter(|_| view.screen == previous_screen) else {
        return Ok(());
    };
    restore_view_state(terminal, view, word_separators)?;
    let current = image_placeholder_at(
        terminal,
        PointerCellEvent {
            column: previous_hover.start,
            row: previous_hover.row,
            click_count: 1,
            rectangle: false,
        },
        terminal.cols()?,
        bound_pasted_images,
    )?;
    if current.as_ref() == Some(&previous_hover) {
        view.active_mut().hover_link = current;
    }
    Ok(())
}

fn reconcile_view_screen_inner(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<bool, WorkerError> {
    if view.copy_mode.is_some() {
        return Ok(false);
    }
    let screen = terminal.active_screen()?;
    if !view.switch_screen(screen) {
        return Ok(false);
    }
    terminal.set_selection(None)?;
    restore_view_state(terminal, view, word_separators)?;
    Ok(true)
}

fn activate_view(
    terminal: &mut Terminal<'_, '_>,
    view_id: TerminalViewId,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    if active.contains_key(&view_id) {
        return Ok(());
    }

    let screen = terminal.active_screen()?;
    let mut state = inactive
        .remove(&view_id)
        .unwrap_or_else(|| Box::new(TerminalViewState::for_screen(screen)));
    state.switch_screen(screen);
    restore_view_state(terminal, &mut state, word_separators)?;
    refresh_frozen_view_appearance(terminal, &mut state)?;
    active.insert(view_id, state);
    Ok(())
}

fn deactivate_view(
    terminal: &mut Terminal<'_, '_>,
    view_id: TerminalViewId,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let Some(mut state) = active.remove(&view_id) else {
        return Ok(());
    };
    restore_view_state(terminal, &mut state, word_separators)?;
    sync_viewport_anchor(terminal, &mut state)?;
    terminal.set_selection(None)?;
    terminal.scroll_viewport(ScrollViewport::Bottom);
    inactive.insert(view_id, state);
    Ok(())
}

fn release_view(
    terminal: &mut Terminal<'_, '_>,
    view_id: TerminalViewId,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
) -> Result<bool, WorkerError> {
    inactive.remove(&view_id);
    if active.remove(&view_id).is_none() {
        return Ok(false);
    }
    terminal.set_selection(None)?;
    terminal.scroll_viewport(ScrollViewport::Bottom);
    Ok(true)
}

#[cfg(unix)]
fn view_takes_direct_input(view: &TerminalViewState) -> bool {
    view.copy_mode.is_none()
        && view.selection.is_none()
        && view.search.is_none()
        && view.search_origin.is_none()
        && view.search_snapshot.is_none()
        && view.hover_link.is_none()
        && matches!(view.viewport, ViewportAnchor::FollowBottom)
        && view.unseen_output == 0
        && !view.mouse_button_pressed
}

fn prepare_live_input(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<bool, WorkerError> {
    if view.copy_mode.is_some() {
        return Ok(false);
    }

    let scrollbar = terminal.scrollbar()?;
    let mut changed = scrollbar.offset.saturating_add(scrollbar.len) < scrollbar.total;
    if changed {
        terminal.scroll_viewport(ScrollViewport::Bottom);
    }
    if view.selection.take().is_some() {
        terminal.set_selection(None)?;
        changed = true;
    }
    if view.search.take().is_some() {
        changed = true;
    }
    view.search_origin = None;
    if view.hover_link.take().is_some() {
        changed = true;
    }
    view.search_snapshot = None;
    view.viewport = ViewportAnchor::FollowBottom;
    view.unseen_output = 0;
    view.mouse_button_pressed = false;
    Ok(changed)
}

fn apply_view_action(
    terminal: &mut Terminal<'_, '_>,
    view_id: TerminalViewId,
    view: &mut TerminalViewState,
    action: TerminalViewAction,
    geometry: Geometry,
    writer: &mut dyn Write,
    mouse_encoder: &mut mouse::Encoder<'_>,
    mouse_event: &mut mouse::Event<'_>,
    input_bytes: &mut Vec<u8>,
    search_worker: &mut SearchWorker,
    wrap_search: bool,
    mode_keys_vi: bool,
    word_separators: &WordSeparators,
    bound_pasted_images: &HashSet<u32>,
    pending_copy_source: &mut Option<Box<CapturedCopySource>>,
    pane_search: &mut Option<CopyModeSearch>,
) -> Result<ViewActionResult, WorkerError> {
    let view_screen = view.screen;
    let TerminalScreenViewState {
        viewport: _,
        selection,
        copy_mode,
        search,
        search_origin,
        search_snapshot,
        hover_link,
        unseen_output,
        mouse_button_pressed,
    } = view.active_mut();
    let hover_cleared = !matches!(
        &action,
        TerminalViewAction::Mouse(_) | TerminalViewAction::CopySelection { .. }
    ) && hover_link.take().is_some();
    let result = match action {
        TerminalViewAction::ScrollWheel { lines, input } => {
            if let Some(mode) = copy_mode.as_mut() {
                scroll_copy_mode(mode, i64::from(lines));
                Ok(ViewActionResult::Snapshot)
            } else {
                let route = wheel_route(terminal, input.force_selection())?;
                match route {
                    WheelRoute::ApplicationMouse => {
                        let selection_changed = selection.take().is_some();
                        if selection_changed {
                            terminal.set_selection(None)?;
                        }
                        for _ in 0..lines.unsigned_abs().min(MAX_WHEEL_REPEAT) {
                            let _ = route_mouse_input(
                                terminal,
                                selection,
                                hover_link,
                                None,
                                input,
                                writer,
                                mouse_encoder,
                                mouse_event,
                                mouse_button_pressed,
                                input_bytes,
                                word_separators,
                                bound_pasted_images,
                            )?;
                        }
                        Ok(if selection_changed {
                            ViewActionResult::OverlaySnapshot
                        } else {
                            ViewActionResult::None
                        })
                    }
                    WheelRoute::AlternateScroll => {
                        let selection_changed = selection.take().is_some();
                        terminal.set_selection(None)?;
                        write_alternate_scroll(terminal, lines, writer)?;
                        Ok(if selection_changed {
                            ViewActionResult::OverlaySnapshot
                        } else {
                            ViewActionResult::None
                        })
                    }
                    WheelRoute::Viewport => {
                        let scrollbar = terminal.scrollbar()?;
                        if scrollbar.total > scrollbar.len {
                            terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(
                                i64::from(lines),
                            )));
                            Ok(ViewActionResult::Snapshot)
                        } else {
                            Ok(ViewActionResult::None)
                        }
                    }
                }
            }
        }
        TerminalViewAction::ScrollLines(lines) => {
            if let Some(mode) = copy_mode.as_mut() {
                scroll_copy_mode(mode, i64::from(lines));
            } else {
                terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(i64::from(lines))));
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollPages(pages) => {
            let rows = copy_mode.as_ref().map_or_else(
                || i64::from(geometry.rows),
                |mode| i64::from(mode.revision.viewport_rows),
            );
            let delta = i64::from(pages).saturating_mul(rows);
            if let Some(mode) = copy_mode.as_mut() {
                scroll_copy_mode(mode, delta);
            } else {
                terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(delta)));
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollTop => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.viewport_offset = 0;
            } else {
                terminal.scroll_viewport(ScrollViewport::Top);
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollBottom => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.viewport_offset = mode.revision.maximum_offset();
            } else {
                terminal.scroll_viewport(ScrollViewport::Bottom);
                *unseen_output = 0;
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollToFraction(fraction) => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.viewport_offset = u32::try_from(
                    u128::from(mode.revision.maximum_offset()).saturating_mul(u128::from(fraction))
                        / u128::from(u32::MAX),
                )
                .unwrap_or(mode.revision.maximum_offset());
                return Ok(ViewActionResult::Snapshot);
            }
            let scrollbar = terminal.scrollbar()?;
            let maximum = scrollbar.total.saturating_sub(scrollbar.len);
            let target = u64::try_from(
                u128::from(maximum).saturating_mul(u128::from(fraction)) / u128::from(u32::MAX),
            )
            .unwrap_or(maximum);
            let delta = i128::from(target).saturating_sub(i128::from(scrollbar.offset));
            terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize_i128(delta)));
            if target == maximum {
                *unseen_output = 0;
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollToOffset(target) => {
            scroll_to_offset(terminal, copy_mode, unseen_output, target)?;
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::SelectionPress(event) => {
            if let Some(mode) = copy_mode.as_mut() {
                mode_selection_press(mode, event, word_separators);
            } else {
                selection_press(terminal, selection, event, word_separators)?;
            }
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::SelectionDrag(event) => {
            if let Some(mode) = copy_mode.as_mut() {
                mode_selection_drag(mode, event, word_separators);
            } else {
                selection_drag(terminal, selection, event, word_separators)?;
            }
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::SelectionAutoscroll { lines, pointer } => {
            if let Some(mode) = copy_mode.as_mut() {
                scroll_copy_mode(mode, i64::from(lines));
                mode_selection_drag(mode, pointer, word_separators);
            } else {
                terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(i64::from(lines))));
                selection_drag(terminal, selection, pointer, word_separators)?;
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::SelectionRelease(_) | TerminalViewAction::ClearLinkHover => {
            Ok(ViewActionResult::None)
        }
        TerminalViewAction::Mouse(input) => route_mouse_input(
            terminal,
            selection,
            hover_link,
            copy_mode.as_deref_mut(),
            input,
            writer,
            mouse_encoder,
            mouse_event,
            mouse_button_pressed,
            input_bytes,
            word_separators,
            bound_pasted_images,
        ),
        TerminalViewAction::SelectAll => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.selection = Some(ModeSelection {
                    anchor: PointCoordinate { x: 0, y: 0 },
                    focus: PointCoordinate {
                        x: mode.revision.columns.saturating_sub(1),
                        y: mode.revision.total_rows().saturating_sub(1),
                    },
                    mode: SelectionMode::Cell,
                    rectangle: false,
                });
                mode.selecting = true;
            } else {
                select_all_history(terminal, selection)?;
            }
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::ClearHistory => {
            search_worker.cancel(view_id);
            terminal.vt_write(b"\x1b[3J");
            *selection = None;
            *copy_mode = None;
            *search = None;
            *search_origin = None;
            *search_snapshot = None;
            *hover_link = None;
            *unseen_output = 0;
            terminal.set_selection(None)?;
            terminal.scroll_viewport(ScrollViewport::Bottom);
            Ok(ViewActionResult::ContentSnapshot)
        }
        TerminalViewAction::ClearSelection => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.selection = None;
                mode.selecting = false;
            } else {
                *selection = None;
                terminal.set_selection(None)?;
            }
            Ok(ViewActionResult::OverlaySnapshot)
        }
        enter_action @ (TerminalViewAction::EnterCopyMode
        | TerminalViewAction::EnterCopyModeScrollExit
        | TerminalViewAction::EnterCopyModeWith { .. }) => {
            let fresh = copy_mode.is_none();
            if fresh {
                drop_view_search(
                    view_id,
                    search_worker,
                    search,
                    search_origin,
                    search_snapshot,
                );
            }
            enter_copy_mode(
                terminal,
                selection,
                copy_mode,
                matches!(
                    enter_action,
                    TerminalViewAction::EnterCopyModeScrollExit
                        | TerminalViewAction::EnterCopyModeWith {
                            scroll_exit: true,
                            ..
                        }
                ),
                matches!(
                    enter_action,
                    TerminalViewAction::EnterCopyModeWith {
                        hide_position: true,
                        ..
                    }
                ),
                pending_copy_source.take(),
                mode_keys_vi,
            )?;
            if fresh && let Some(mode) = copy_mode.as_deref_mut() {
                mode.search = pane_search.clone().map(|stored| CopyModeSearch {
                    direction: SearchDirection::Backward,
                    ..stored
                });
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::CopyModeCounted { action, count } => {
            let revision_before = copy_mode.as_ref().map(|mode| mode.revision.id);
            let result = if let CopyModeAction::Search(spec) = &action {
                run_copy_mode_search(
                    view_id,
                    copy_mode,
                    search,
                    search_snapshot,
                    search_worker,
                    spec,
                    count,
                    mode_keys_vi,
                    wrap_search,
                    pane_search,
                );
                ViewActionResult::Snapshot
            } else if let CopyModeAction::SearchAgain { reverse } = action {
                if let Some(spec) = copy_mode_search_again(copy_mode.as_deref(), reverse) {
                    run_copy_mode_search(
                        view_id,
                        copy_mode,
                        search,
                        search_snapshot,
                        search_worker,
                        &spec,
                        count,
                        mode_keys_vi,
                        wrap_search,
                        pane_search,
                    );
                } else {
                    let forward = search
                        .as_ref()
                        .is_none_or(|search| search.query.direction == SearchDirection::Forward);
                    let direction = if forward ^ reverse { 1 } else { -1 };
                    for _ in 0..count {
                        step_search(search, direction, wrap_search);
                    }
                    sync_copy_cursor_to_search(copy_mode, search.as_deref());
                    reveal_search_for_view(terminal, copy_mode.as_deref(), search.as_deref())?;
                }
                ViewActionResult::Snapshot
            } else {
                apply_counted_copy_mode_action(
                    terminal,
                    selection,
                    copy_mode,
                    unseen_output,
                    action,
                    count,
                    word_separators,
                    mode_keys_vi,
                )?
            };
            settle_copy_search(
                view_id,
                search_worker,
                copy_mode,
                search,
                search_origin,
                search_snapshot,
                revision_before,
            );
            Ok(result)
        }
        TerminalViewAction::CopyMode(CopyModeAction::Search(spec)) => {
            run_copy_mode_search(
                view_id,
                copy_mode,
                search,
                search_snapshot,
                search_worker,
                &spec,
                1,
                mode_keys_vi,
                wrap_search,
                pane_search,
            );
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::CopyMode(CopyModeAction::SearchAgain { reverse }) => {
            if let Some(spec) = copy_mode_search_again(copy_mode.as_deref(), reverse) {
                run_copy_mode_search(
                    view_id,
                    copy_mode,
                    search,
                    search_snapshot,
                    search_worker,
                    &spec,
                    1,
                    mode_keys_vi,
                    wrap_search,
                    pane_search,
                );
            } else {
                let forward = search
                    .as_ref()
                    .is_none_or(|search| search.query.direction == SearchDirection::Forward);
                let direction = if forward ^ reverse { 1 } else { -1 };
                step_search(search, direction, wrap_search);
                sync_copy_cursor_to_search(copy_mode, search.as_deref());
                reveal_search_for_view(terminal, copy_mode.as_deref(), search.as_deref())?;
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::CopyMode(CopyModeAction::SearchCursorWord { direction }) => {
            let text = copy_mode
                .as_deref()
                .and_then(|mode| mode_cursor_word(&mode.revision, mode.cursor, word_separators));
            if let Some(text) = text {
                run_copy_mode_search(
                    view_id,
                    copy_mode,
                    search,
                    search_snapshot,
                    search_worker,
                    &CopyModeSearch {
                        text,
                        direction,
                        regex: false,
                        incremental: false,
                    },
                    1,
                    mode_keys_vi,
                    wrap_search,
                    pane_search,
                );
                Ok(ViewActionResult::Snapshot)
            } else {
                Ok(ViewActionResult::None)
            }
        }
        TerminalViewAction::CopyMode(action) => {
            let revision_before = copy_mode.as_ref().map(|mode| mode.revision.id);
            let result = apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                action,
                word_separators,
                mode_keys_vi,
            )?;
            settle_copy_search(
                view_id,
                search_worker,
                copy_mode,
                search,
                search_origin,
                search_snapshot,
                revision_before,
            );
            Ok(result)
        }
        TerminalViewAction::SearchBegin(query) => {
            remember_copy_mode_search(copy_mode.as_deref_mut(), pane_search, &query);
            let selection =
                search_selection_policy(copy_mode.as_deref(), search_origin, query.direction, true);
            if search_snapshot.is_none()
                && let Some(mode) = copy_mode.as_ref()
            {
                *search_snapshot = Some(Arc::clone(&mode.revision.search));
            }
            queue_search(
                terminal,
                SearchTarget {
                    view_id,
                    screen: view_screen,
                    search,
                    snapshot: search_snapshot,
                },
                search_worker,
                query,
                selection,
            )?;
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::SearchUpdate(query) => {
            remember_copy_mode_search(copy_mode.as_deref_mut(), pane_search, &query);
            let selection = search_selection_policy(
                copy_mode.as_deref(),
                search_origin,
                query.direction,
                false,
            );
            if search_snapshot.is_none()
                && let Some(mode) = copy_mode.as_ref()
            {
                *search_snapshot = Some(Arc::clone(&mode.revision.search));
            }
            queue_search(
                terminal,
                SearchTarget {
                    view_id,
                    screen: view_screen,
                    search,
                    snapshot: search_snapshot,
                },
                search_worker,
                query,
                selection,
            )?;
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::SearchNext => {
            step_search(search, 1, wrap_search);
            sync_copy_cursor_to_search(copy_mode, search.as_deref());
            reveal_search_for_view(terminal, copy_mode.as_deref(), search.as_deref())?;
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::SearchPrevious => {
            step_search(search, -1, wrap_search);
            sync_copy_cursor_to_search(copy_mode, search.as_deref());
            reveal_search_for_view(terminal, copy_mode.as_deref(), search.as_deref())?;
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::SearchClose => {
            search_worker.cancel(view_id);
            *search = None;
            *search_origin = None;
            *search_snapshot = None;
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::CopySelection { request_id, target } => {
            let text = copy_mode.as_ref().map_or_else(
                || format_selection_text(terminal),
                |mode| {
                    Ok(mode.selection.map_or_else(String::new, |selection| {
                        mode.revision.format_selection(selection, mode_keys_vi)
                    }))
                },
            )?;
            Ok(ViewActionResult::Copy(Box::new(TerminalCopyReady {
                request_id,
                clipboard: Some(target),
                buffer: None,
                pipe: None,
                text,
                view_changed: false,
            })))
        }
        TerminalViewAction::Focus(focused) => {
            write_focus_event(terminal, focused, writer)?;
            Ok(ViewActionResult::None)
        }
        TerminalViewAction::Paste(text) => {
            write_paste_bytes(terminal, text.into_bytes(), true, writer)?;
            Ok(ViewActionResult::None)
        }
    };
    let result = match warn_and_skip_view_ghostty("view action", result)? {
        Some(result) => result,
        None if hover_cleared => return Ok(ViewActionResult::OverlaySnapshot),
        None => return Ok(ViewActionResult::None),
    };
    Ok(
        if hover_cleared && matches!(result, ViewActionResult::None) {
            ViewActionResult::OverlaySnapshot
        } else {
            result
        },
    )
}

fn write_paste_bytes(
    terminal: &Terminal<'_, '_>,
    mut source: Vec<u8>,
    bracketed: bool,
    writer: &mut dyn Write,
) -> Result<(), WorkerError> {
    if source.is_empty() {
        return Ok(());
    }
    let mut encoded = vec![0_u8; source.len().saturating_add(16)];
    let bracketed = bracketed && terminal.mode(Mode::BRACKETED_PASTE)?;
    let written = libghostty_vt::paste::encode(&mut source, bracketed, &mut encoded)?;
    writer.write_all(&encoded[..written])?;
    writer.flush()?;
    Ok(())
}

fn write_prepared_paste_bytes(
    terminal: &Terminal<'_, '_>,
    source: &[u8],
    bracketed: bool,
    writer: &mut dyn Write,
) -> Result<(), WorkerError> {
    let bracketed = bracketed && terminal.mode(Mode::BRACKETED_PASTE)?;
    if source.is_empty() && !bracketed {
        return Ok(());
    }
    if bracketed {
        writer.write_all(b"\x1b[200~")?;
    }
    writer.write_all(source)?;
    if bracketed {
        writer.write_all(b"\x1b[201~")?;
    }
    writer.flush()?;
    Ok(())
}

fn scroll_copy_mode(mode: &mut CopyModeState, delta: i64) {
    let next = i128::from(mode.viewport_offset).saturating_add(i128::from(delta));
    mode.viewport_offset = u32::try_from(next.max(0))
        .unwrap_or(u32::MAX)
        .min(mode.revision.maximum_offset());
}

fn mode_pointer_point(mode: &CopyModeState, event: PointerCellEvent) -> PointCoordinate {
    mode.revision.clamp_point(PointCoordinate {
        x: event.column,
        y: mode.viewport_offset.saturating_add(u32::from(event.row)),
    })
}

fn mode_selection_press(
    mode: &mut CopyModeState,
    event: PointerCellEvent,
    word_separators: &WordSeparators,
) {
    let point = mode_pointer_point(mode, event);
    mode.cursor = point;
    mode.rectangle = event.rectangle;
    let selection_mode = match event.click_count {
        0 | 1 => SelectionMode::Cell,
        2 => SelectionMode::Word,
        _ => SelectionMode::Line,
    };
    let (anchor, focus) = match selection_mode {
        SelectionMode::Cell => (point, point),
        SelectionMode::Word => mode_word_bounds(&mode.revision, point, word_separators),
        SelectionMode::Line => (
            PointCoordinate { x: 0, y: point.y },
            PointCoordinate {
                x: mode.revision.columns.saturating_sub(1),
                y: point.y,
            },
        ),
    };
    mode.selection = Some(ModeSelection {
        anchor,
        focus,
        mode: selection_mode,
        rectangle: mode.rectangle,
    });
    mode.selecting = true;
}

fn mode_selection_drag(
    mode: &mut CopyModeState,
    event: PointerCellEvent,
    word_separators: &WordSeparators,
) {
    let point = mode_pointer_point(mode, event);
    mode.cursor = point;
    if let Some(selection) = mode.selection.as_mut() {
        selection.focus = match selection.mode {
            SelectionMode::Cell => point,
            SelectionMode::Word => {
                let (start, end) = mode_word_bounds(&mode.revision, point, word_separators);
                if (point.y, point.x) < (selection.anchor.y, selection.anchor.x) {
                    start
                } else {
                    end
                }
            }
            SelectionMode::Line => PointCoordinate {
                x: mode.revision.columns.saturating_sub(1),
                y: point.y,
            },
        };
        mode.rectangle |= event.rectangle;
        selection.rectangle = mode.rectangle;
    }
}

fn mode_word_edge(
    revision: &ModeRevision,
    point: PointCoordinate,
    word_separators: &WordSeparators,
    forward: bool,
) -> PointCoordinate {
    let mut at = point;
    while revision_cell_is_whitespace(revision, at) {
        if forward {
            if at.x.saturating_add(1) >= revision.columns {
                return point;
            }
            at.x += 1;
        } else {
            if at.x == 0 {
                return point;
            }
            at.x -= 1;
        }
    }
    let (start, end) = mode_word_bounds(revision, at, word_separators);
    if forward { end } else { start }
}

fn mode_word_bounds(
    revision: &ModeRevision,
    point: PointCoordinate,
    word_separators: &WordSeparators,
) -> (PointCoordinate, PointCoordinate) {
    let class = revision_word_class(revision, point, word_separators);
    let mut start = point.x;
    while start > 0
        && revision_word_class(
            revision,
            PointCoordinate {
                x: start - 1,
                y: point.y,
            },
            word_separators,
        ) == class
    {
        start -= 1;
    }
    let mut end = point.x;
    while end + 1 < revision.columns
        && revision_word_class(
            revision,
            PointCoordinate {
                x: end + 1,
                y: point.y,
            },
            word_separators,
        ) == class
    {
        end += 1;
    }
    (
        PointCoordinate {
            x: start,
            y: point.y,
        },
        PointCoordinate { x: end, y: point.y },
    )
}

fn mode_cursor_word(
    revision: &ModeRevision,
    point: PointCoordinate,
    word_separators: &WordSeparators,
) -> Option<String> {
    if revision_cell_is_whitespace(revision, point) {
        return None;
    }
    let (anchor, focus) = mode_word_bounds(revision, point, word_separators);
    let text = revision.format_selection(
        ModeSelection {
            anchor,
            focus,
            mode: SelectionMode::Word,
            rectangle: false,
        },
        true,
    );
    (!text.is_empty()).then_some(text)
}

fn capture_copy_source(terminal: &mut Terminal<'_, '_>) -> Result<CapturedCopySource, WorkerError> {
    let revision = ModeRevision::capture(terminal)?;
    let last_used = used_screen_rows(terminal).unwrap_or(0);
    let cursor_row = terminal.cursor_y()?;
    let (x, y) = if cursor_row + 1 > last_used {
        (0, last_used.saturating_sub(1))
    } else {
        (
            terminal.cursor_x()?.min(terminal.cols()?.saturating_sub(1)),
            cursor_row,
        )
    };
    let cursor = terminal.grid_ref(Point::Active(PointCoordinate { x, y: u32::from(y) }))?;
    let cursor = terminal
        .point_from_grid_ref(&cursor, PointSpace::Screen)?
        .unwrap_or(PointCoordinate { x, y: u32::from(y) });
    let viewport_offset = revision.maximum_offset();
    Ok(CapturedCopySource {
        revision,
        cursor,
        viewport_offset,
    })
}

fn enter_copy_mode(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    copy_mode: &mut CopyModeSlot,
    scroll_exit: bool,
    hide_position: bool,
    source: Option<Box<CapturedCopySource>>,
    mode_keys_vi: bool,
) -> Result<(), WorkerError> {
    if copy_mode.is_some() {
        return Ok(());
    }
    terminal.scroll_viewport(ScrollViewport::Bottom);
    terminal.set_selection(None)?;
    *selection = None;
    let sourced = source.is_some();
    let (revision, cursor, viewport_offset) = if let Some(source) = source {
        let CapturedCopySource {
            revision,
            cursor,
            viewport_offset,
        } = *source;
        (revision, cursor, viewport_offset)
    } else {
        let x = terminal.cursor_x()?.min(terminal.cols()?.saturating_sub(1));
        let y = terminal.cursor_y()?;
        let cursor = terminal.grid_ref(Point::Active(PointCoordinate { x, y: u32::from(y) }))?;
        let cursor = terminal
            .point_from_grid_ref(&cursor, PointSpace::Screen)?
            .unwrap_or(PointCoordinate { x, y: u32::from(y) });
        let revision = ModeRevision::capture(terminal)?;
        let viewport_offset = u32::try_from(terminal.scrollbar()?.offset).unwrap_or(u32::MAX);
        (revision, cursor, viewport_offset)
    };
    *copy_mode = Some(Box::new(CopyModeState {
        revision,
        cursor,
        viewport_offset,
        scroll_exit,
        hide_position,
        selection: None,
        selection_mode: CopySelectionMode::Char,
        selection_origin: PointCoordinate { x: 0, y: 0 },
        recentre: None,
        mark: None,
        last_jump: None,
        selecting: false,
        rectangle: false,
        refresh: false,
        sourced,
        kind: FrozenModeKind::Copy,
        last_cx: 0,
        last_sx: 0,
        mode_keys_vi_at_entry: mode_keys_vi,
        search_marks: false,
        search_count: Some((0, false)),
        incremental_origin: None,
        search: None,
        search_all: true,
        line_numbers: 1,
    }));
    Ok(())
}

fn drop_view_search(
    view_id: TerminalViewId,
    search_worker: &mut SearchWorker,
    search: &mut SearchSlot,
    search_origin: &mut Option<PointCoordinate>,
    search_snapshot: &mut Option<Arc<HistorySearchSnapshot>>,
) {
    search_worker.cancel(view_id);
    *search = None;
    *search_origin = None;
    *search_snapshot = None;
}

fn wheel_route(
    terminal: &Terminal<'_, '_>,
    force_selection: bool,
) -> Result<WheelRoute, WorkerError> {
    if force_selection {
        return Ok(WheelRoute::Viewport);
    }
    if terminal.is_mouse_tracking()? {
        return Ok(WheelRoute::ApplicationMouse);
    }
    if terminal.active_screen()? == Screen::Alternate && terminal.mode(Mode::ALT_SCROLL)? {
        return Ok(WheelRoute::AlternateScroll);
    }
    Ok(WheelRoute::Viewport)
}

fn write_alternate_scroll(
    terminal: &Terminal<'_, '_>,
    lines: i32,
    writer: &mut dyn Write,
) -> Result<(), WorkerError> {
    let sequence = if terminal.mode(Mode::DECCKM)? {
        if lines < 0 { b"\x1bOA" } else { b"\x1bOB" }
    } else if lines < 0 {
        b"\x1b[A"
    } else {
        b"\x1b[B"
    };
    let repetitions = usize::try_from(lines.unsigned_abs().min(MAX_WHEEL_REPEAT)).unwrap_or(0);
    let mut bytes = [0_u8; 96];
    let length = repetitions.saturating_mul(sequence.len());
    for chunk in bytes[..length].chunks_exact_mut(sequence.len()) {
        chunk.copy_from_slice(sequence);
    }
    if length != 0 {
        writer.write_all(&bytes[..length])?;
        writer.flush()?;
    }
    Ok(())
}

#[allow(
    clippy::cast_precision_loss,
    reason = "libghostty's mouse ABI uses f32 pixel positions"
)]
fn route_mouse_input(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    hover_link: &mut Option<HoverLink>,
    copy_mode: Option<&mut CopyModeState>,
    input: TerminalMouseInput,
    writer: &mut dyn Write,
    encoder: &mut mouse::Encoder<'_>,
    event: &mut mouse::Event<'_>,
    button_pressed: &mut bool,
    input_bytes: &mut Vec<u8>,
    word_separators: &WordSeparators,
    bound_pasted_images: &HashSet<u32>,
) -> Result<ViewActionResult, WorkerError> {
    let phase = input.phase();
    let button = input.button();
    let modifiers = input.modifiers();
    let force_selection = input.force_selection();
    if let Some(mode) = copy_mode {
        *hover_link = None;
        if button != Some(TerminalMouseButton::Left) {
            return Ok(ViewActionResult::None);
        }
        match phase {
            TerminalMousePhase::Press => {
                mode_selection_press(mode, input.cell, word_separators);
            }
            TerminalMousePhase::Motion => {
                mode_selection_drag(mode, input.cell, word_separators);
            }
            TerminalMousePhase::Release => {}
        }
        return Ok(ViewActionResult::OverlaySnapshot);
    }
    if phase == TerminalMousePhase::Press
        && button == Some(TerminalMouseButton::Left)
        && input.cell.click_count >= 3
        && (modifiers.control() || modifiers.platform())
        && select_semantic_output_at(terminal, selection, input.cell)?
    {
        *hover_link = None;
        return Ok(ViewActionResult::OverlaySnapshot);
    }

    let link_modifier = modifiers.control() || modifiers.platform();
    let plain_image_motion = !force_selection
        && modifiers.bits() == 0
        && phase == TerminalMousePhase::Motion
        && button.is_none();
    let plain_image_hover_changed = if plain_image_motion {
        let columns = terminal.cols()?;
        let next_hover = image_placeholder_at(terminal, input.cell, columns, bound_pasted_images)?;
        let changed = *hover_link != next_hover;
        *hover_link = next_hover;
        Some(changed)
    } else {
        None
    };

    if !force_selection && terminal.is_mouse_tracking()? {
        let hover_changed =
            plain_image_hover_changed.unwrap_or_else(|| hover_link.take().is_some());
        let is_wheel = matches!(
            button,
            Some(TerminalMouseButton::ScrollUp | TerminalMouseButton::ScrollDown)
        );
        *button_pressed = match phase {
            TerminalMousePhase::Press if !is_wheel => true,
            TerminalMousePhase::Press | TerminalMousePhase::Release => false,
            TerminalMousePhase::Motion => *button_pressed,
        };
        let mut modifiers = key::Mods::empty();
        modifiers.set(key::Mods::SHIFT, input.modifiers().shift());
        modifiers.set(key::Mods::CTRL, input.modifiers().control());
        modifiers.set(key::Mods::ALT, input.modifiers().alt());
        modifiers.set(key::Mods::SUPER, input.modifiers().platform());
        event
            .set_action(match phase {
                TerminalMousePhase::Press => mouse::Action::Press,
                TerminalMousePhase::Release => mouse::Action::Release,
                TerminalMousePhase::Motion => mouse::Action::Motion,
            })
            .set_button(button.map(|button| match button {
                TerminalMouseButton::Left => mouse::Button::Left,
                TerminalMouseButton::Middle => mouse::Button::Middle,
                TerminalMouseButton::Right => mouse::Button::Right,
                TerminalMouseButton::ScrollUp => mouse::Button::Four,
                TerminalMouseButton::ScrollDown => mouse::Button::Five,
            }))
            .set_mods(modifiers)
            .set_position(mouse::Position {
                x: input.x as f32,
                y: input.y as f32,
            });
        encoder
            .set_options_from_terminal(terminal)
            .set_size(EncoderSize {
                screen_width: input.screen_width,
                screen_height: input.screen_height,
                cell_width: input.cell_width.max(1),
                cell_height: input.cell_height.max(1),
                padding_top: 0,
                padding_bottom: 0,
                padding_right: 0,
                padding_left: 0,
            })
            .set_any_button_pressed(*button_pressed)
            .set_track_last_cell(true);
        input_bytes.clear();
        encoder.encode_to_vec(event, input_bytes)?;
        if !input_bytes.is_empty() {
            writer.write_all(input_bytes)?;
            writer.flush()?;
        }
        return Ok(if hover_changed {
            ViewActionResult::OverlaySnapshot
        } else {
            ViewActionResult::None
        });
    }

    if !force_selection
        && phase == TerminalMousePhase::Press
        && button == Some(TerminalMouseButton::Left)
        && (modifiers.control() || modifiers.platform())
    {
        if let Some(link) = hover_link.as_ref().filter(|link| {
            link.contains(input.cell) && hover_link_is_bound(link, bound_pasted_images)
        }) {
            return Ok(ViewActionResult::OpenUri(link.uri.clone()));
        }
        if let Some(link) = hover_link_at(terminal, input.cell, input_bytes, bound_pasted_images)? {
            let uri = link.uri.clone();
            *hover_link = Some(link);
            return Ok(ViewActionResult::OpenUri(uri));
        }
    }

    let keep_cached_hover = !plain_image_motion
        && link_modifier
        && phase == TerminalMousePhase::Motion
        && button.is_none()
        && hover_link
            .as_ref()
            .is_some_and(|link| link.contains(input.cell));
    let hover_changed = if let Some(changed) = plain_image_hover_changed {
        changed
    } else if keep_cached_hover {
        false
    } else {
        let next_hover = (link_modifier && phase == TerminalMousePhase::Motion && button.is_none())
            .then(|| hover_link_at(terminal, input.cell, input_bytes, bound_pasted_images))
            .transpose()?
            .flatten();
        let changed = *hover_link != next_hover;
        if phase == TerminalMousePhase::Motion {
            *hover_link = next_hover;
        } else if phase == TerminalMousePhase::Press {
            *hover_link = None;
        }
        changed
    };
    if button != Some(TerminalMouseButton::Left) {
        return Ok(if hover_changed {
            ViewActionResult::OverlaySnapshot
        } else {
            ViewActionResult::None
        });
    }
    match phase {
        TerminalMousePhase::Press => {
            selection_press(terminal, selection, input.cell, word_separators)?;
        }
        TerminalMousePhase::Motion => {
            selection_drag(terminal, selection, input.cell, word_separators)?;
        }
        TerminalMousePhase::Release => {}
    }
    Ok(ViewActionResult::OverlaySnapshot)
}

fn hover_link_at(
    terminal: &Terminal<'_, '_>,
    point: PointerCellEvent,
    scratch: &mut Vec<u8>,
    bound_pasted_images: &HashSet<u32>,
) -> Result<Option<HoverLink>, WorkerError> {
    let columns = terminal.cols()?;
    let rows = terminal.rows()?;
    if point.column >= columns || point.row >= rows {
        return Ok(None);
    }

    if let Some(length) = hyperlink_uri_bytes(terminal, point.row, point.column, scratch)? {
        let Ok(uri) = std::str::from_utf8(&scratch[..length]) else {
            return Ok(None);
        };
        if !is_safe_link_uri(uri) {
            return Ok(None);
        }
        let uri = uri.to_owned();

        let mut start = point.column;
        while start > 0
            && cell_has_hyperlink_uri(terminal, point.row, start - 1, uri.as_bytes(), scratch)?
        {
            start -= 1;
        }
        let mut end = point.column.saturating_add(1);
        while end < columns
            && cell_has_hyperlink_uri(terminal, point.row, end, uri.as_bytes(), scratch)?
        {
            end += 1;
        }

        return Ok(Some(HoverLink {
            row: point.row,
            start,
            end,
            uri,
        }));
    }

    if let Some(link) = image_placeholder_at(terminal, point, columns, bound_pasted_images)? {
        return Ok(Some(link));
    }

    plain_uri_at(terminal, point, columns, scratch)
}

fn image_placeholder_at(
    terminal: &Terminal<'_, '_>,
    point: PointerCellEvent,
    columns: u16,
    bound_pasted_images: &HashSet<u32>,
) -> Result<Option<HoverLink>, WorkerError> {
    let rows = terminal.rows()?;
    if point.column >= columns || point.row >= rows {
        return Ok(None);
    }
    let point_index = usize::from(point.column);
    let window_start = point_index.saturating_sub(MAX_PLACEHOLDER_CELLS - 1);
    let window_end = point_index
        .saturating_add(MAX_PLACEHOLDER_CELLS)
        .min(usize::from(columns));
    let mut cells = [b' '; 2 * MAX_PLACEHOLDER_CELLS - 1];
    for (cell, column) in cells.iter_mut().zip(window_start..window_end) {
        *cell = ascii_terminal_cell(
            terminal,
            point.row,
            u16::try_from(column).unwrap_or(u16::MAX),
        )?;
    }
    let cells = &cells[..window_end.saturating_sub(window_start)];
    let pointer = point_index.saturating_sub(window_start);

    for (prefix, suffix) in IMAGE_PLACEHOLDERS {
        for start in (0..=pointer.min(cells.len().saturating_sub(1))).rev() {
            let Some(rest) = cells[start..].strip_prefix(prefix.as_bytes()) else {
                continue;
            };
            let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
            if !rest[digits..].starts_with(suffix.as_bytes()) {
                continue;
            }
            let Some(number) = std::str::from_utf8(&rest[..digits])
                .ok()
                .and_then(|digits| digits.parse::<u32>().ok())
            else {
                continue;
            };
            if !bound_pasted_images.contains(&number) {
                continue;
            }
            let end = start + prefix.len() + digits + suffix.len();
            if pointer >= end {
                continue;
            }
            return Ok(Some(HoverLink {
                row: point.row,
                start: u16::try_from(window_start + start).unwrap_or(u16::MAX),
                end: u16::try_from(window_start + end).unwrap_or(u16::MAX),
                uri: format!("{IMAGE_PLACEHOLDER_SCHEME}://{number}"),
            }));
        }
    }
    Ok(None)
}

fn hover_link_is_bound(link: &HoverLink, bound_pasted_images: &HashSet<u32>) -> bool {
    pasted_image_number(&link.uri).is_none_or(|number| bound_pasted_images.contains(&number))
}

fn pasted_image_number(uri: &str) -> Option<u32> {
    uri.strip_prefix(IMAGE_PLACEHOLDER_SCHEME)?
        .strip_prefix("://")?
        .parse()
        .ok()
}

fn image_placeholder_occurrences(
    terminal: &Terminal<'_, '_>,
) -> Result<HashMap<u32, usize>, WorkerError> {
    let columns = terminal.cols()?;
    let rows = terminal.rows()?;
    let first_row = u32::try_from(terminal.scrollback_rows()?).unwrap_or(u32::MAX);
    let mut occurrences = HashMap::new();
    let mut cells = Vec::with_capacity(usize::from(columns));
    for row in 0..rows {
        cells.clear();
        for column in 0..columns {
            cells.push(ascii_terminal_screen_cell(
                terminal,
                first_row.saturating_add(u32::from(row)),
                column,
            )?);
        }
        for start in 0..cells.len() {
            for (prefix, suffix) in IMAGE_PLACEHOLDERS {
                let Some(rest) = cells[start..].strip_prefix(prefix.as_bytes()) else {
                    continue;
                };
                let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
                if !rest[digits..].starts_with(suffix.as_bytes()) {
                    continue;
                }
                let Some(number) = std::str::from_utf8(&rest[..digits])
                    .ok()
                    .and_then(|digits| digits.parse::<u32>().ok())
                else {
                    continue;
                };
                *occurrences.entry(number).or_insert(0) += 1;
                break;
            }
        }
    }
    Ok(occurrences)
}

fn plain_uri_at(
    terminal: &Terminal<'_, '_>,
    point: PointerCellEvent,
    columns: u16,
    scratch: &mut Vec<u8>,
) -> Result<Option<HoverLink>, WorkerError> {
    let started = diagnostic_timer();
    scratch.clear();
    let point_index = usize::from(point.column);
    let result = (|| {
        if point.column >= columns
            || ascii_terminal_cell(terminal, point.row, point.column)?.is_ascii_whitespace()
        {
            return Ok(None);
        }

        let mut start = point_index;
        while start > 0 && point_index.saturating_sub(start) < MAX_LINK_URI_BYTES - 1 {
            let previous = ascii_terminal_cell(
                terminal,
                point.row,
                u16::try_from(start - 1).unwrap_or_default(),
            )?;
            if previous.is_ascii_whitespace() {
                break;
            }
            start -= 1;
        }
        let mut end = point_index.saturating_add(1);
        while end < usize::from(columns) && end.saturating_sub(start) < MAX_LINK_URI_BYTES {
            let next =
                ascii_terminal_cell(terminal, point.row, u16::try_from(end).unwrap_or(u16::MAX))?;
            if next.is_ascii_whitespace() {
                break;
            }
            end += 1;
        }
        if (start > 0
            && !ascii_terminal_cell(
                terminal,
                point.row,
                u16::try_from(start - 1).unwrap_or_default(),
            )?
            .is_ascii_whitespace())
            || (end < usize::from(columns)
                && !ascii_terminal_cell(
                    terminal,
                    point.row,
                    u16::try_from(end).unwrap_or(u16::MAX),
                )?
                .is_ascii_whitespace())
        {
            return Ok(None);
        }

        scratch.reserve(end.saturating_sub(start));
        for column in start..end {
            scratch.push(ascii_terminal_cell(
                terminal,
                point.row,
                u16::try_from(column).unwrap_or(u16::MAX),
            )?);
        }
        let mut local_start = 0;
        let mut local_end = scratch.len();
        while local_start < local_end
            && matches!(
                scratch[local_start],
                b'(' | b'[' | b'{' | b'<' | b'\'' | b'"'
            )
        {
            local_start += 1;
        }
        while local_start < local_end
            && matches!(
                scratch[local_end - 1],
                b'.' | b',' | b';' | b'!' | b'?' | b')' | b']' | b'}' | b'>' | b'\'' | b'"'
            )
        {
            local_end -= 1;
        }
        let local_point = point_index.saturating_sub(start);
        if !(local_start..local_end).contains(&local_point) {
            return Ok(None);
        }
        let Ok(uri) = std::str::from_utf8(&scratch[local_start..local_end]) else {
            return Ok(None);
        };
        if !is_safe_link_uri(uri) {
            return Ok(None);
        }
        Ok(Some(HoverLink {
            row: point.row,
            start: u16::try_from(start.saturating_add(local_start)).unwrap_or(u16::MAX),
            end: u16::try_from(start.saturating_add(local_end)).unwrap_or(u16::MAX),
            uri: uri.to_owned(),
        }))
    })();
    log::trace!(
        target: "zz_terminal::diagnostics::link",
        "plain_uri_lookup row={} column={} columns={} scanned_bytes={} hit={} elapsed_us={}",
        point.row,
        point.column,
        columns,
        scratch.len(),
        result.as_ref().is_ok_and(Option::is_some),
        diagnostic_elapsed_us(started),
    );
    result
}

fn ascii_terminal_cell(
    terminal: &Terminal<'_, '_>,
    row: u16,
    column: u16,
) -> Result<u8, WorkerError> {
    let grid_ref = terminal.grid_ref(Point::Viewport(PointCoordinate {
        x: column,
        y: u32::from(row),
    }))?;
    let codepoint = grid_ref.cell()?.codepoint()?;
    Ok(u8::try_from(codepoint)
        .ok()
        .filter(u8::is_ascii_graphic)
        .unwrap_or(b' '))
}

fn ascii_terminal_screen_cell(
    terminal: &Terminal<'_, '_>,
    row: u32,
    column: u16,
) -> Result<u8, WorkerError> {
    let grid_ref = terminal.grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?;
    let codepoint = grid_ref.cell()?.codepoint()?;
    Ok(u8::try_from(codepoint)
        .ok()
        .filter(u8::is_ascii_graphic)
        .unwrap_or(b' '))
}

fn cell_has_hyperlink_uri(
    terminal: &Terminal<'_, '_>,
    row: u16,
    column: u16,
    expected: &[u8],
    scratch: &mut Vec<u8>,
) -> Result<bool, WorkerError> {
    Ok(hyperlink_uri_bytes(terminal, row, column, scratch)?
        .is_some_and(|length| scratch[..length] == *expected))
}

fn hyperlink_uri_bytes(
    terminal: &Terminal<'_, '_>,
    row: u16,
    column: u16,
    scratch: &mut Vec<u8>,
) -> Result<Option<usize>, WorkerError> {
    hyperlink_uri_bytes_at(
        terminal,
        Point::Viewport(PointCoordinate {
            x: column,
            y: u32::from(row),
        }),
        scratch,
    )
}

fn hyperlink_uri_bytes_at(
    terminal: &Terminal<'_, '_>,
    point: Point,
    scratch: &mut Vec<u8>,
) -> Result<Option<usize>, WorkerError> {
    if scratch.len() < LINK_URI_SCRATCH_BYTES {
        scratch.resize(LINK_URI_SCRATCH_BYTES, 0);
    }
    let grid_ref = terminal.grid_ref(point)?;
    match grid_ref.hyperlink_uri(scratch) {
        Ok(0) => Ok(None),
        Ok(length) => Ok(Some(length)),
        Err(libghostty_vt::Error::OutOfSpace { required })
            if required > 0 && required <= MAX_LINK_URI_BYTES =>
        {
            scratch.resize(required, 0);
            let length = grid_ref.hyperlink_uri(scratch)?;
            Ok((length > 0).then_some(length))
        }
        Err(libghostty_vt::Error::OutOfSpace { .. }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn is_safe_link_uri(uri: &str) -> bool {
    if uri.len() > MAX_LINK_URI_BYTES
        || uri
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return false;
    }
    let Some((scheme, rest)) = uri.split_once(':') else {
        return false;
    };
    if scheme.eq_ignore_ascii_case("mailto") {
        return !rest.is_empty();
    }
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "file" | "ssh"
    ) && rest.starts_with("//")
}

fn history_scrollbar_state(
    terminal: &Terminal<'_, '_>,
) -> Result<ScrollbarState, TerminalCaptureError> {
    let scrollbar = terminal.scrollbar().map_err(capture_failure)?;
    Ok(ScrollbarState {
        total: u32::try_from(scrollbar.total).map_err(|_| {
            TerminalCaptureError::Failed("terminal history is too large".to_owned())
        })?,
        offset: u32::try_from(scrollbar.offset).map_err(|_| {
            TerminalCaptureError::Failed("terminal history is too large".to_owned())
        })?,
        len: u32::try_from(scrollbar.len).map_err(|_| {
            TerminalCaptureError::Failed("terminal history is too large".to_owned())
        })?,
    })
}

fn capture_history(
    terminal: &Terminal<'_, '_>,
    start: u32,
    count: u32,
    hints: &ClassHints,
    bound: Option<(u16, u16)>,
) -> Result<HistoryCapture, TerminalCaptureError> {
    let history_rows =
        u32::try_from(terminal.scrollback_rows().map_err(capture_failure)?).unwrap_or(u32::MAX);
    let start = start.min(history_rows);
    let end = start.saturating_add(count).min(history_rows);
    let (columns, _) = bounded_extent((terminal.cols().map_err(capture_failure)?.max(1), 1), bound);
    let foreground = terminal
        .fg_color()
        .map_err(capture_failure)?
        .unwrap_or_default();
    let background = terminal
        .bg_color()
        .map_err(capture_failure)?
        .unwrap_or_default();
    let palette = terminal.color_palette().map_err(capture_failure)?.0;
    let default_palette = terminal.default_color_palette().map_err(capture_failure)?.0;
    let classes = Classifier::new(hints, &palette, &default_palette, [foreground, background]);
    let default_style = classes.default_style(PackedStyle::new(
        color(foreground),
        color(background),
        None,
        0,
        UnderlineStyle::None,
    ));
    let mut dictionary = ViewportDictionary::default();
    dictionary.ensure_default(default_style, &palette);
    let mut rows = Vec::with_capacity(usize::try_from(end - start).unwrap_or(0));
    let mut stack = ['\0'; 8];
    let mut grapheme_scratch = Vec::new();
    let mut grapheme_text = String::with_capacity(8);

    for row in start..end {
        let mut output = Vec::with_capacity(usize::from(columns));
        for column in 0..columns {
            let grid_ref = terminal
                .grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))
                .map_err(capture_failure)?;
            let raw_style = grid_ref.style().map_err(capture_failure)?;
            let raw_cell = grid_ref.cell().map_err(capture_failure)?;
            let mut cell_foreground = resolve_style_color(raw_style.fg_color, &palette)
                .unwrap_or_else(|| color(foreground));
            let mut cell_background = resolve_style_color(raw_style.bg_color, &palette)
                .unwrap_or_else(|| color(background));
            cell_background = match raw_cell.content_tag().map_err(capture_failure)? {
                CellContentTag::BgColorPalette => color(
                    palette[usize::from(raw_cell.bg_color_palette().map_err(capture_failure)?.0)],
                ),
                CellContentTag::BgColorRgb => {
                    color(raw_cell.bg_color_rgb().map_err(capture_failure)?)
                }
                CellContentTag::Codepoint | CellContentTag::CodepointGrapheme => cell_background,
            };
            let classes = if raw_style.inverse {
                (ColourClass::Resolved, ColourClass::Resolved)
            } else {
                (
                    classes.ground(raw_style.fg_color, 0),
                    match raw_cell.content_tag().map_err(capture_failure)? {
                        CellContentTag::BgColorPalette => {
                            classes.entry(raw_cell.bg_color_palette().map_err(capture_failure)?.0)
                        }
                        CellContentTag::BgColorRgb => ColourClass::Rgb,
                        CellContentTag::Codepoint | CellContentTag::CodepointGrapheme => {
                            classes.ground(raw_style.bg_color, 1)
                        }
                    },
                )
            };
            if raw_style.inverse {
                std::mem::swap(&mut cell_foreground, &mut cell_background);
            }

            grapheme_text.clear();
            match grid_ref.graphemes(&mut stack) {
                Ok(count) => grapheme_text.extend(stack[..count].iter().copied()),
                Err(libghostty_vt::Error::OutOfSpace { required }) => {
                    if grapheme_scratch.len() < required {
                        grapheme_scratch.resize(required, '\0');
                    }
                    let count = grid_ref
                        .graphemes(&mut grapheme_scratch)
                        .map_err(capture_failure)?;
                    grapheme_text.extend(grapheme_scratch[..count].iter().copied());
                }
                Err(error) => return Err(capture_failure(error)),
            }

            let width = match raw_cell.wide().map_err(capture_failure)? {
                CellWide::Narrow => CellWidth::Narrow,
                CellWide::Wide => CellWidth::Wide,
                CellWide::SpacerTail => CellWidth::SpacerTail,
                CellWide::SpacerHead => CellWidth::SpacerHead,
            };
            let foreground_explicit_rgb = matches!(
                if raw_style.inverse {
                    raw_style.bg_color
                } else {
                    raw_style.fg_color
                },
                StyleColor::Rgb(_)
            );
            let style = PackedStyle::new(
                cell_foreground,
                cell_background,
                resolve_style_color(raw_style.underline_color, &palette),
                style_attributes(
                    &raw_style,
                    foreground_explicit_rgb,
                    raw_cell.has_hyperlink().map_err(capture_failure)?,
                ),
                underline_style(raw_style.underline),
            )
            .with_classes(classes.0, classes.1);
            output.push(PackedCell::new(
                dictionary.encode_glyph(&grapheme_text),
                dictionary.intern_style(style),
                width,
            ));
        }
        rows.push(output);
    }

    Ok((
        start,
        rows,
        dictionary.shared_dictionary().as_ref().clone(),
        history_scrollbar_state(terminal)?,
        columns,
    ))
}

#[cfg(test)]
#[path = "session/capture_work_tests.rs"]
mod capture_work_tests;

#[cfg(test)]
#[path = "session/capture_links_tests.rs"]
mod capture_links_tests;

const CAPTURE_ROWS_PER_STEP: u64 = 512;

trait CaptureGrid: GridRead {
    fn capture_total(&self) -> Result<usize, libghostty_vt::Error>;
    fn capture_rows(&self) -> Result<u16, libghostty_vt::Error>;
    fn capture_columns(&self) -> Result<u16, libghostty_vt::Error>;
    fn capture_history_rows(&self) -> Result<usize, libghostty_vt::Error>;
    fn capture_screen(&self) -> Result<Screen, libghostty_vt::Error>;
    fn capture_format(
        &self,
        options: FormatterOptions<'_, '_>,
    ) -> Result<String, TerminalCaptureError>;
}

impl CaptureGrid for Terminal<'_, '_> {
    fn capture_total(&self) -> Result<usize, libghostty_vt::Error> {
        self.total_rows()
    }
    fn capture_rows(&self) -> Result<u16, libghostty_vt::Error> {
        self.rows()
    }
    fn capture_columns(&self) -> Result<u16, libghostty_vt::Error> {
        self.cols()
    }
    fn capture_history_rows(&self) -> Result<usize, libghostty_vt::Error> {
        self.scrollback_rows()
    }
    fn capture_screen(&self) -> Result<Screen, libghostty_vt::Error> {
        self.active_screen()
    }
    fn capture_format(
        &self,
        options: FormatterOptions<'_, '_>,
    ) -> Result<String, TerminalCaptureError> {
        let mut formatter = Formatter::new(self, options).map_err(capture_failure)?;
        let length = match formatter.format_len() {
            Ok(length) => length,
            Err(libghostty_vt::Error::InvalidValue) => return Ok(String::new()),
            Err(error) => return Err(capture_failure(error)),
        };
        if length > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
        if length == 0 {
            return Ok(String::new());
        }
        let mut output = vec![0_u8; length];
        let written = formatter.format_buf(&mut output).map_err(capture_failure)?;
        output.truncate(written);
        String::from_utf8(output).map_err(|error| TerminalCaptureError::Failed(error.to_string()))
    }
}

impl CaptureGrid for ScreenSnapshot {
    fn capture_total(&self) -> Result<usize, libghostty_vt::Error> {
        self.total_rows()
    }
    fn capture_rows(&self) -> Result<u16, libghostty_vt::Error> {
        self.rows()
    }
    fn capture_columns(&self) -> Result<u16, libghostty_vt::Error> {
        self.cols()
    }
    fn capture_history_rows(&self) -> Result<usize, libghostty_vt::Error> {
        Ok(self.total_rows()?.saturating_sub(usize::from(self.rows()?)))
    }
    fn capture_screen(&self) -> Result<Screen, libghostty_vt::Error> {
        self.active_screen()
    }
    fn capture_format(
        &self,
        options: FormatterOptions<'_, '_>,
    ) -> Result<String, TerminalCaptureError> {
        let bytes = match self.format_alloc(options) {
            Ok(bytes) => bytes,
            Err(libghostty_vt::Error::InvalidValue) => return Ok(String::new()),
            Err(error) => return Err(capture_failure(error)),
        };
        if bytes.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
        String::from_utf8(bytes.to_vec())
            .map_err(|error| TerminalCaptureError::Failed(error.to_string()))
    }
}

enum CaptureSource {
    Screen(ScreenSnapshot),
    Mode(Arc<ModeRevision>),
}

struct CaptureWork {
    source: CaptureSource,
    options: CaptureOptions,
    output_rows: Vec<u64>,
    next: u64,
    end: u64,
    visible_start: u64,
    output: String,
    separator: bool,
    previous: CaptureCarry,
    reply: ActorReply<Result<String, TerminalCaptureError>>,
}

#[derive(Default)]
struct CaptureCarry {
    style: libghostty_vt::style::Style,
    link: Option<Vec<u8>>,
    idle_prompt: Option<u64>,
}

impl CaptureWork {
    fn start(
        terminal: &Terminal<'_, '_>,
        mode: Option<&CopyModeState>,
        filter: &EngineFilter,
        request: CaptureRequest,
    ) -> Option<Self> {
        let CaptureRequest { options, reply } = request;
        let options = CaptureOptions {
            mode: options.mode && mode.is_some(),
            ..options
        };
        let output_rows = if options.line_flags {
            filter.output_rows(terminal)
        } else {
            Vec::new()
        };
        let dimensions = if options.mode
            && let Some(mode) = mode
        {
            Ok((
                u64::from(mode.revision.total_rows()),
                u64::from(mode.revision.viewport_rows),
                u64::from(mode.revision.maximum_offset()),
            ))
        } else {
            terminal
                .total_rows()
                .and_then(|total| {
                    Ok((
                        total as u64,
                        u64::from(terminal.rows()?),
                        terminal.scrollback_rows()? as u64,
                    ))
                })
                .map_err(capture_failure)
        };
        let result = dimensions.and_then(|(total, rows, visible_start)| {
            let visible_end = visible_start
                .saturating_add(rows.saturating_sub(1))
                .min(total.saturating_sub(1));
            let start = resolve_capture_boundary(options.start, visible_start, visible_end, total);
            let end = resolve_capture_boundary(options.end, visible_start, visible_end, total);
            if start > end || end - start < CAPTURE_ROWS_PER_STEP || options.hyperlinks {
                return capture_terminal_marked(terminal, mode, options, &output_rows).map(Err);
            }
            let source = if options.mode
                && let Some(mode) = mode
            {
                if options.alternate && mode.revision.screen != Screen::Alternate {
                    return Err(TerminalCaptureError::AlternateUnavailable);
                }
                CaptureSource::Mode(Arc::clone(&mode.revision))
            } else {
                if options.alternate
                    && terminal.active_screen().map_err(capture_failure)? != Screen::Alternate
                {
                    return Err(TerminalCaptureError::AlternateUnavailable);
                }
                CaptureSource::Screen(terminal.clone_screen().map_err(capture_failure)?)
            };
            let idle_prompt = idle_prompt_row(terminal, options)?;
            Ok(Ok((source, start, end, visible_start, idle_prompt)))
        });
        match result {
            Ok(Ok((source, next, end, visible_start, idle_prompt))) => Some(Self {
                source,
                options,
                output_rows,
                next,
                end,
                visible_start,
                output: String::new(),
                separator: false,
                previous: CaptureCarry {
                    idle_prompt,
                    ..CaptureCarry::default()
                },
                reply,
            }),
            result => {
                let _ = reply.send(match result {
                    Ok(Err(output)) => Ok(output),
                    Err(error) => Err(error),
                    Ok(Ok(_)) => unreachable!(),
                });
                None
            }
        }
    }

    fn wrapped(&self, row: u64) -> Result<bool, TerminalCaptureError> {
        match &self.source {
            CaptureSource::Screen(screen) => screen
                .grid_ref(Point::Screen(PointCoordinate {
                    x: 0,
                    y: u32::try_from(row).unwrap_or(u32::MAX),
                }))
                .and_then(|grid| grid.row())
                .and_then(libghostty_vt::screen::Row::is_wrapped)
                .map_err(capture_failure),
            CaptureSource::Mode(revision) => Ok(revision
                .row(u32::try_from(row).unwrap_or(u32::MAX))
                .wrapped()),
        }
    }

    fn step(&mut self) -> Result<bool, TerminalCaptureError> {
        let start = self.next;
        let end = start
            .saturating_add(CAPTURE_ROWS_PER_STEP - 1)
            .min(self.end);
        let options = CaptureOptions {
            start: CaptureBoundary::Relative(start as i64 - self.visible_start as i64),
            end: CaptureBoundary::Relative(end as i64 - self.visible_start as i64),
            ..self.options
        };
        let continuing = end < self.end && self.options.join_wrapped && self.wrapped(end)?;
        let keep_tail = continuing
            && !options.number_lines
            && !options.line_flags
            && !options.escape_sequences
            && !options.preserve_trailing;
        let mut output = match &self.source {
            CaptureSource::Screen(screen) => capture_grid(
                screen,
                CaptureOptions {
                    preserve_trailing: options.preserve_trailing || keep_tail,
                    ..options
                },
                &mut self.previous,
                &self.output_rows,
            )?,
            CaptureSource::Mode(revision) => capture_revision(
                revision,
                self.visible_start as u32,
                options,
                &mut self.previous,
            )?,
        };
        if keep_tail && matches!(self.source, CaptureSource::Screen(_)) {
            let mut lines = output.split('\n').peekable();
            let mut trimmed = String::with_capacity(output.len());
            while let Some(line) = lines.next() {
                if lines.peek().is_some() {
                    trimmed.push_str(line.trim_end());
                    trimmed.push('\n');
                } else {
                    trimmed.push_str(line);
                }
            }
            output = trimmed;
        }
        if self.separator {
            self.output.push('\n');
        }
        self.output.push_str(&output);
        if self.output.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
        self.separator = !(self.options.join_wrapped && self.wrapped(end)?);
        self.next = end + 1;
        Ok(end == self.end)
    }
}

fn step_capture_work(work: &mut VecDeque<CaptureWork>) {
    let Some(mut capture) = work.pop_front() else {
        return;
    };
    match capture.step() {
        Ok(true) => {
            let _ = capture.reply.send(Ok(capture.output));
        }
        Ok(false) => work.push_front(capture),
        Err(error) => {
            let _ = capture.reply.send(Err(error));
        }
    }
}

fn stamp_copy_mode_marks(
    terminal: &Terminal<'_, '_>,
    filter: &EngineFilter,
    views: &ActiveTerminalViews,
) {
    for mode in views.values().filter_map(|view| view.copy_mode.as_deref()) {
        mode.revision
            .stamp_output_rows(|| filter.output_rows(terminal));
    }
}

fn capture_terminal(
    terminal: &Terminal<'_, '_>,
    mode: Option<&CopyModeState>,
    options: CaptureOptions,
) -> Result<String, TerminalCaptureError> {
    capture_terminal_marked(terminal, mode, options, &[])
}

fn capture_terminal_marked(
    terminal: &Terminal<'_, '_>,
    mode: Option<&CopyModeState>,
    options: CaptureOptions,
    output_rows: &[u64],
) -> Result<String, TerminalCaptureError> {
    if options.mode
        && let Some(mode) = mode
    {
        if options.alternate && mode.revision.screen != Screen::Alternate {
            return Err(TerminalCaptureError::AlternateUnavailable);
        }
        if options.hyperlinks {
            return Ok(String::new());
        }
        return capture_mode_revision(mode, options);
    }
    capture_grid(
        terminal,
        CaptureOptions {
            mode: false,
            ..options
        },
        &mut CaptureCarry {
            idle_prompt: idle_prompt_row(terminal, options)?,
            ..CaptureCarry::default()
        },
        output_rows,
    )
}

fn idle_prompt_row(
    terminal: &Terminal<'_, '_>,
    options: CaptureOptions,
) -> Result<Option<u64>, TerminalCaptureError> {
    let find = || -> Result<Option<u64>, libghostty_vt::Error> {
        let mut row = u32::try_from(terminal.scrollback_rows()?)
            .unwrap_or(u32::MAX)
            .saturating_add(u32::from(terminal.cursor_y()?));
        let cursor = terminal.grid_row(Point::Screen(PointCoordinate { x: 0, y: row }))?;
        if cursor.row()?.semantic_prompt()? != RowSemanticPrompt::Continuation {
            return Ok(None);
        }
        for column in 0..terminal.cols()? {
            if let Some(cell) = cursor.cell(column)
                && cell.cell()?.has_text()?
            {
                return Ok(None);
            }
        }
        while let Some(above) = row.checked_sub(1) {
            row = above;
            match terminal
                .grid_ref(Point::Screen(PointCoordinate { x: 0, y: row }))?
                .row()?
                .semantic_prompt()?
            {
                RowSemanticPrompt::Continuation => {}
                RowSemanticPrompt::Prompt => return Ok(Some(u64::from(row))),
                RowSemanticPrompt::None => return Ok(None),
            }
        }
        Ok(None)
    };
    if options.skip_input && options.keep_idle_prompt {
        find().map_err(capture_failure)
    } else {
        Ok(None)
    }
}

fn capture_grid(
    terminal: &impl CaptureGrid,
    options: CaptureOptions,
    previous: &mut CaptureCarry,
    output_rows: &[u64],
) -> Result<String, TerminalCaptureError> {
    let active_screen = terminal.capture_screen().map_err(capture_failure)?;
    if options.alternate && active_screen != Screen::Alternate {
        return Err(TerminalCaptureError::AlternateUnavailable);
    }
    let total = terminal.capture_total().map_err(capture_failure)?;
    if total == 0 {
        return Ok(String::new());
    }
    let total = u64::try_from(total).unwrap_or(u64::MAX);
    let rows = u64::from(terminal.capture_rows().map_err(capture_failure)?);
    let visible_start = u64::try_from(terminal.capture_history_rows().map_err(capture_failure)?)
        .unwrap_or(u64::MAX)
        .min(total.saturating_sub(1));
    let visible_end = visible_start
        .saturating_add(rows.saturating_sub(1))
        .min(total.saturating_sub(1));
    let start = resolve_capture_boundary(options.start, visible_start, visible_end, total);
    let end = resolve_capture_boundary(options.end, visible_start, visible_end, total);
    if start > end {
        return Ok(String::new());
    }
    let requested_rows = usize::try_from(end.saturating_sub(start).saturating_add(1)).unwrap_or(1);

    let columns = terminal.capture_columns().map_err(capture_failure)?;
    if options.hyperlinks {
        return capture_hyperlinks(
            terminal,
            options,
            (start, end),
            visible_start,
            columns,
            output_rows,
        );
    }
    if options.escape_sequences {
        return capture_styled_terminal(
            terminal,
            options,
            (start, end),
            visible_start,
            columns,
            previous,
            output_rows,
        );
    }
    let head = terminal
        .grid_ref(Point::Screen(PointCoordinate {
            x: 0,
            y: u32::try_from(start).unwrap_or(u32::MAX),
        }))
        .map_err(capture_failure)?;
    let tail = terminal
        .grid_ref(Point::Screen(PointCoordinate {
            x: columns.saturating_sub(1),
            y: u32::try_from(end).unwrap_or(u32::MAX),
        }))
        .map_err(capture_failure)?;
    let selection = Selection::new(head, tail, false);
    let format_range = |join_wrapped: bool| -> Result<String, TerminalCaptureError> {
        let formatter_options = FormatterOptions::new()
            .with_format(Format::Plain)
            .with_unwrap(join_wrapped)
            .with_trim(!options.preserve_trailing)
            .with_selection(&selection);
        terminal.capture_format(formatter_options)
    };
    let per_row = options.number_lines || options.line_flags;
    let output = format_range(options.join_wrapped && !per_row)?;
    let output = if options.skip_input {
        blank_prompt_lines(
            terminal,
            &output,
            (start, end),
            options.join_wrapped && !per_row,
            previous.idle_prompt,
        )?
    } else {
        output
    };
    let rows = if per_row && options.join_wrapped {
        Some(
            (start..=end)
                .map(|row| {
                    terminal
                        .grid_ref(Point::Screen(PointCoordinate {
                            x: 0,
                            y: u32::try_from(row).unwrap_or(u32::MAX),
                        }))
                        .and_then(|grid| grid.row())
                        .and_then(libghostty_vt::screen::Row::is_wrapped)
                        .map_err(capture_failure)
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };
    let written_rows = if options.join_wrapped {
        measure_written_rows(terminal, &selection)?
    } else if output.is_empty() {
        0
    } else {
        output.split('\n').count()
    };
    let trailing_rows = requested_rows.saturating_sub(written_rows);
    let output = pad_capture_rows(&output, trailing_rows, columns, options);
    let flags = if options.line_flags {
        Some(
            (start..=end)
                .map(|row| grid_line_flags(terminal, row, output_rows))
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };
    Ok(decorate_capture(
        output,
        rows.as_deref(),
        options,
        start,
        visible_start,
        flags.as_deref(),
    ))
}

fn blank_prompt_lines(
    terminal: &impl CaptureGrid,
    output: &str,
    (start, end): (u64, u64),
    joined: bool,
    idle_prompt: Option<u64>,
) -> Result<String, TerminalCaptureError> {
    let row_at = |row: u64| {
        terminal
            .grid_ref(Point::Screen(PointCoordinate {
                x: 0,
                y: u32::try_from(row).unwrap_or(u32::MAX),
            }))
            .and_then(|grid| grid.row())
            .map_err(capture_failure)
    };
    let mut row = start;
    let lines = output.split('\n').collect::<Vec<_>>();
    let mut kinds = Vec::with_capacity(lines.len());
    for _ in &lines {
        kinds.push(if row <= end {
            let first = row_at(row)?;
            let kind = match first.semantic_prompt().map_err(capture_failure)? {
                RowSemanticPrompt::Prompt if idle_prompt == Some(row) => {
                    RowSemanticPrompt::Continuation
                }
                kind => kind,
            };
            let mut wrapped = first.is_wrapped().map_err(capture_failure)?;
            row += 1;
            while joined && wrapped && row <= end {
                wrapped = row_at(row)?.is_wrapped().map_err(capture_failure)?;
                row += 1;
            }
            kind
        } else {
            RowSemanticPrompt::None
        });
    }
    let mut output_follows = false;
    let mut input = vec![false; lines.len()];
    for (index, kind) in kinds.iter().enumerate().rev() {
        input[index] = match kind {
            RowSemanticPrompt::None => {
                output_follows = true;
                false
            }
            RowSemanticPrompt::Prompt => {
                output_follows = false;
                true
            }
            RowSemanticPrompt::Continuation => output_follows,
        };
    }
    let mut blanked = String::with_capacity(output.len());
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            blanked.push('\n');
        }
        if !input[index] {
            blanked.push_str(line);
        }
    }
    Ok(blanked)
}

const LINE_FLAG_HYPERLINK: u8 = 1 << 0;
const LINE_FLAG_OUTPUT: u8 = 1 << 1;
const LINE_FLAG_PROMPT: u8 = 1 << 2;
const LINE_FLAG_WRAPPED: u8 = 1 << 3;
const MAX_HYPERLINK_URI: usize = 1024;

fn push_line_flags(output: &mut String, flags: u8) {
    for (bit, letter) in [
        (LINE_FLAG_HYPERLINK, 'H'),
        (LINE_FLAG_OUTPUT, 'O'),
        (LINE_FLAG_PROMPT, 'P'),
        (LINE_FLAG_WRAPPED, 'W'),
    ] {
        if flags & bit != 0 {
            output.push(letter);
        }
    }
    if flags == 0 {
        output.push('-');
    }
    output.push(' ');
}

fn grid_line_flags(
    terminal: &impl GridRead,
    row: u64,
    output_rows: &[u64],
) -> Result<u8, TerminalCaptureError> {
    let line = terminal
        .grid_ref(Point::Screen(PointCoordinate {
            x: 0,
            y: u32::try_from(row).unwrap_or(u32::MAX),
        }))
        .and_then(|grid| grid.row())
        .map_err(capture_failure)?;
    let mut flags = 0;
    if line.has_hyperlink().map_err(capture_failure)? {
        flags |= LINE_FLAG_HYPERLINK;
    }
    if output_rows.binary_search(&row).is_ok() {
        flags |= LINE_FLAG_OUTPUT;
    }
    if line.semantic_prompt().map_err(capture_failure)? == RowSemanticPrompt::Prompt {
        flags |= LINE_FLAG_PROMPT;
    }
    if line.is_wrapped().map_err(capture_failure)? {
        flags |= LINE_FLAG_WRAPPED;
    }
    Ok(flags)
}

fn capture_hyperlinks(
    terminal: &impl GridRead,
    options: CaptureOptions,
    (start, end): (u64, u64),
    history_rows: u64,
    columns: u16,
    output_rows: &[u64],
) -> Result<String, TerminalCaptureError> {
    let mut output = String::new();
    let mut found = 0_usize;
    let mut carried: Option<Vec<u8>> = None;
    let mut buffer = vec![0_u8; 256];
    for row in start..=end {
        let y = u32::try_from(row).unwrap_or(u32::MAX);
        let line = terminal
            .grid_ref(Point::Screen(PointCoordinate { x: 0, y }))
            .and_then(|grid| grid.row())
            .map_err(capture_failure)?;
        let wrapped = line.is_wrapped().map_err(capture_failure)?;
        let mut current = carried.take();
        if !line.has_hyperlink().map_err(capture_failure)? {
            continue;
        }
        let mut uris = Vec::new();
        for x in 0..columns {
            let grid = terminal
                .grid_ref(Point::Screen(PointCoordinate { x, y }))
                .map_err(capture_failure)?;
            let cell = grid.cell().map_err(capture_failure)?;
            if matches!(
                cell.wide().map_err(capture_failure)?,
                CellWide::SpacerTail | CellWide::SpacerHead
            ) {
                continue;
            }
            if !cell.has_hyperlink().map_err(capture_failure)? {
                current = None;
                continue;
            }
            let length = capture_link_uri(&grid, &mut buffer)?;
            let uri = &buffer[..length];
            if current.as_deref() == Some(uri) {
                continue;
            }
            let visible = visible_uri(uri);
            if uri.is_empty() || visible.len() > MAX_HYPERLINK_URI {
                current = None;
                continue;
            }
            current = Some(uri.to_vec());
            if found == usize::from(columns) {
                break;
            }
            found += 1;
            uris.push(visible);
        }
        if wrapped {
            carried = current;
        }
        if uris.is_empty() {
            continue;
        }
        if options.number_lines {
            push_capture_line_number(&mut output, row, history_rows);
        }
        if options.line_flags {
            push_line_flags(&mut output, grid_line_flags(terminal, row, output_rows)?);
        }
        output.push_str(&uris.join(" "));
        if !(options.join_wrapped && wrapped) {
            output.push('\n');
        }
        if output.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
    }
    if output.ends_with('\n') {
        output.pop();
    }
    Ok(output)
}

fn visible_uri(uri: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(uri.len());
    for chunk in uri.utf8_chunks() {
        for character in chunk.valid().chars() {
            match character {
                '\\' => output.push_str("\\\\"),
                '\t' | '\n' => output.push(character),
                character if character.is_ascii_control() => {
                    let _ = write!(output, "\\{:03o}", u32::from(character));
                }
                character => output.push(character),
            }
        }
        for byte in chunk.invalid() {
            let _ = write!(output, "\\{byte:03o}");
        }
    }
    output
}

fn capture_styled_terminal(
    terminal: &impl GridRead,
    options: CaptureOptions,
    (start, end): (u64, u64),
    history_rows: u64,
    columns: u16,
    previous: &mut CaptureCarry,
    output_rows: &[u64],
) -> Result<String, TerminalCaptureError> {
    let escaped = options.escape_nonprintable;
    let mut output = String::new();
    let mut graphemes = vec!['\0'; 8];
    let mut uri = vec![0_u8; 256];
    for row in start..=end {
        let y = u32::try_from(row).unwrap_or(u32::MAX);
        let mut used = 0;
        let mut text_end = 0;
        for x in 0..columns {
            let cell = terminal
                .grid_ref(Point::Screen(PointCoordinate { x, y }))
                .and_then(|grid| grid.cell())
                .map_err(capture_failure)?;
            if cell.has_text().map_err(capture_failure)? {
                text_end = x + 1;
            }
            if cell.has_text().map_err(capture_failure)?
                || cell.has_styling().map_err(capture_failure)?
                || matches!(
                    cell.content_tag().map_err(capture_failure)?,
                    CellContentTag::BgColorPalette | CellContentTag::BgColorRgb
                )
            {
                used = x + 1;
            }
        }
        let width = if options.join_wrapped || options.trim_positions {
            text_end
        } else {
            u16::try_from(allocated_row_width(usize::from(used), usize::from(columns)))
                .unwrap_or(columns)
        };
        let mut line = String::new();
        let mut code = String::new();
        let mut has_link = false;
        for x in 0..width {
            let grid = terminal
                .grid_ref(Point::Screen(PointCoordinate { x, y }))
                .map_err(capture_failure)?;
            let cell = grid.cell().map_err(capture_failure)?;
            let wide = cell.wide().map_err(capture_failure)?;
            if wide == CellWide::SpacerTail
                || (wide == CellWide::SpacerHead && options.join_wrapped)
            {
                continue;
            }
            let mut style = if wide == CellWide::SpacerHead {
                libghostty_vt::style::Style::default()
            } else {
                grid.style().map_err(capture_failure)?
            };
            match cell.content_tag().map_err(capture_failure)? {
                CellContentTag::BgColorPalette => {
                    style.bg_color =
                        StyleColor::Palette(cell.bg_color_palette().map_err(capture_failure)?);
                }
                CellContentTag::BgColorRgb => {
                    style.bg_color = StyleColor::Rgb(cell.bg_color_rgb().map_err(capture_failure)?);
                }
                CellContentTag::Codepoint | CellContentTag::CodepointGrapheme => {}
            }
            code.clear();
            push_capture_sgr(&mut code, previous.style, style);
            if escaped {
                code = code.replace('\u{1b}', "\\033");
            }
            previous.style = style;
            let length = if !options.mode
                && wide != CellWide::SpacerHead
                && cell.has_hyperlink().map_err(capture_failure)?
            {
                capture_link_uri(&grid, &mut uri)?
            } else {
                0
            };
            let cell_uri = (length > 0).then(|| &uri[..length]);
            if cell_uri != previous.link.as_deref() {
                if let Some(visible) = cell_uri
                    .map(visible_uri)
                    .filter(|visible| visible.len() <= MAX_HYPERLINK_URI)
                {
                    push_capture_link(&mut code, &visible, escaped);
                    has_link = true;
                    previous.link = cell_uri.map(<[u8]>::to_vec);
                } else {
                    if has_link {
                        push_capture_link(&mut code, "", escaped);
                        has_link = false;
                    }
                    previous.link = None;
                }
            }
            line.push_str(&code);
            let count = match grid.graphemes(&mut graphemes) {
                Ok(count) => count,
                Err(libghostty_vt::Error::OutOfSpace { required }) => {
                    if required > MAX_CAPTURE_BYTES / std::mem::size_of::<char>() {
                        return Err(TerminalCaptureError::TooLarge);
                    }
                    graphemes.resize(required, '\0');
                    grid.graphemes(&mut graphemes).map_err(capture_failure)?
                }
                Err(error) => return Err(capture_failure(error)),
            };
            if count == 0 {
                line.push(' ');
            } else if escaped && count == 1 && graphemes[0] == '\\' {
                line.push_str("\\\\");
            } else {
                line.extend(graphemes[..count].iter());
            }
        }
        if has_link {
            line.push_str(&code);
            push_capture_link(&mut line, "", escaped);
        }
        if options.number_lines {
            push_capture_line_number(&mut output, row, history_rows);
        }
        if options.line_flags {
            push_line_flags(&mut output, grid_line_flags(terminal, row, output_rows)?);
        }
        if !options.join_wrapped && !options.preserve_trailing {
            output.push_str(line.trim_end_matches(' '));
        } else {
            output.push_str(&line);
        }
        let wrapped = terminal
            .grid_ref(Point::Screen(PointCoordinate { x: 0, y }))
            .and_then(|grid| grid.row())
            .and_then(libghostty_vt::screen::Row::is_wrapped)
            .map_err(capture_failure)?;
        if row < end && !(options.join_wrapped && wrapped) {
            output.push('\n');
        }
        if output.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
    }
    Ok(decorate_capture(
        output,
        None,
        CaptureOptions {
            number_lines: false,
            escape_nonprintable: false,
            ..options
        },
        start,
        history_rows,
        None,
    ))
}

fn capture_link_uri(
    grid: &libghostty_vt::screen::GridRef<'_>,
    buffer: &mut Vec<u8>,
) -> Result<usize, TerminalCaptureError> {
    match grid.hyperlink_uri(buffer) {
        Ok(length) => Ok(length),
        Err(libghostty_vt::Error::OutOfSpace { required }) => {
            buffer.resize(required, 0);
            grid.hyperlink_uri(buffer).map_err(capture_failure)
        }
        Err(error) => Err(capture_failure(error)),
    }
}

fn push_capture_link(output: &mut String, uri: &str, escaped: bool) {
    let escape = if escaped { "\\033" } else { "\u{1b}" };
    output.push_str(escape);
    output.push_str("]8;;");
    output.push_str(uri);
    output.push_str(escape);
    output.push_str(if escaped { "\\\\" } else { "\\" });
}

fn capture_sgr_attributes(style: libghostty_vt::style::Style) -> [(bool, &'static str); 13] {
    [
        (style.bold, "1"),
        (style.faint, "2"),
        (style.italic, "3"),
        (style.underline == Underline::Single, "4"),
        (style.blink, "5"),
        (style.inverse, "7"),
        (style.invisible, "8"),
        (style.strikethrough, "9"),
        (style.underline == Underline::Double, "4:2"),
        (style.underline == Underline::Curly, "4:3"),
        (style.underline == Underline::Dotted, "4:4"),
        (style.underline == Underline::Dashed, "4:5"),
        (style.overline, "5:3"),
    ]
}

fn push_capture_sgr(
    output: &mut String,
    previous: libghostty_vt::style::Style,
    style: libghostty_vt::style::Style,
) {
    let old = capture_sgr_attributes(previous);
    let new = capture_sgr_attributes(style);
    let reset = old
        .iter()
        .zip(&new)
        .any(|((old, _), (new, _))| *old && !new)
        || (previous.underline_color != StyleColor::None
            && style.underline_color == StyleColor::None);
    let mut attributes = Vec::new();
    if reset {
        attributes.push("0");
    }
    for ((old, _), (new, code)) in old.iter().zip(&new) {
        if *new && (reset || !old) {
            attributes.push(*code);
        }
    }
    if !attributes.is_empty() {
        output.push_str("\x1b[");
        output.push_str(&attributes.join(";"));
        output.push('m');
    }
    for (old, new, base) in [
        (previous.fg_color, style.fg_color, 30),
        (previous.bg_color, style.bg_color, 40),
        (previous.underline_color, style.underline_color, 50),
    ] {
        if (reset && new != StyleColor::None) || (!reset && old != new) {
            push_capture_colour(output, new, base);
        }
    }
}

fn push_capture_colour(output: &mut String, colour: StyleColor, base: u16) {
    use std::fmt::Write as _;

    match colour {
        StyleColor::None if base == 50 => {}
        StyleColor::None => {
            let _ = write!(output, "\x1b[{}m", base + 9);
        }
        StyleColor::Palette(index) if base != 50 && index.0 < 8 => {
            let _ = write!(output, "\x1b[{}m", base + u16::from(index.0));
        }
        StyleColor::Palette(index) if base != 50 && index.0 < 16 => {
            let _ = write!(output, "\x1b[{}m", base + 60 + u16::from(index.0) - 8);
        }
        StyleColor::Palette(index) => {
            let _ = write!(output, "\x1b[{};5;{}m", base + 8, index.0);
        }
        StyleColor::Rgb(rgb) => {
            let _ = write!(output, "\x1b[{};2;{};{};{}m", base + 8, rgb.r, rgb.g, rgb.b);
        }
    }
}

fn measure_written_rows(
    terminal: &impl CaptureGrid,
    selection: &Selection<'_>,
) -> Result<usize, TerminalCaptureError> {
    let options = FormatterOptions::new()
        .with_format(Format::Plain)
        .with_unwrap(false)
        .with_trim(true)
        .with_selection(selection);
    let output = terminal.capture_format(options)?;
    Ok(if output.is_empty() {
        0
    } else {
        output.split('\n').count()
    })
}

fn allocated_row_width(used: usize, columns: usize) -> usize {
    if used == 0 {
        return 0;
    }
    if used < columns / 4 {
        columns / 4
    } else if used < columns / 2 {
        columns / 2
    } else if used < columns {
        columns
    } else {
        used
    }
}

fn pad_capture_rows(
    output: &str,
    trailing_rows: usize,
    columns: u16,
    options: CaptureOptions,
) -> String {
    let empty_cells = !options.join_wrapped && !options.trim_positions;
    let trim_spaces = !options.join_wrapped && !options.preserve_trailing;
    let mut lines = if output.is_empty() {
        Vec::new()
    } else {
        output.split('\n').map(str::to_owned).collect::<Vec<_>>()
    };
    if empty_cells && options.preserve_trailing {
        let columns = usize::from(columns);
        for line in &mut lines {
            let width = line.chars().count();
            let allocated = allocated_row_width(width, columns);
            for _ in width..allocated {
                line.push(' ');
            }
        }
    }
    if trim_spaces {
        for line in &mut lines {
            while line.ends_with(' ') {
                line.pop();
            }
        }
    }
    lines.resize(lines.len().saturating_add(trailing_rows), String::new());
    lines.join("\n")
}

fn decorate_capture(
    text: String,
    rows: Option<&[bool]>,
    options: CaptureOptions,
    first_row: u64,
    history_rows: u64,
    flags: Option<&[u8]>,
) -> String {
    let text = if options.number_lines || flags.is_some() {
        number_capture(
            &text,
            rows,
            (first_row, history_rows),
            options.number_lines,
            flags,
        )
    } else {
        text
    };
    if options.escape_nonprintable {
        text.replace('\\', "\\\\").replace('\u{1b}', "\\033")
    } else {
        text
    }
}

fn push_capture_line_number(output: &mut String, row: u64, history_rows: u64) {
    let number = i64::try_from(row)
        .unwrap_or(i64::MAX)
        .saturating_sub(i64::try_from(history_rows).unwrap_or(i64::MAX));
    output.push_str(&number.to_string());
    output.push(' ');
}

fn number_capture(
    text: &str,
    rows: Option<&[bool]>,
    (first_row, history_rows): (u64, u64),
    numbers: bool,
    flags: Option<&[u8]>,
) -> String {
    let mut output = String::with_capacity(text.len());
    for (offset, line) in text.split('\n').enumerate() {
        if offset > 0 && rows.is_none_or(|rows| rows.get(offset - 1) != Some(&true)) {
            output.push('\n');
        }
        if numbers {
            push_capture_line_number(
                &mut output,
                first_row.saturating_add(offset as u64),
                history_rows,
            );
        }
        if let Some(flags) = flags {
            push_line_flags(&mut output, flags.get(offset).copied().unwrap_or(0));
        }
        output.push_str(line);
    }
    output
}

const PANE_RESET_PRELUDE: &[u8] = b"\x1b\\\x1b[m\x1b(B\x1b)B\x1b[r\x1b[?7h\x1b[?25h\x1b[?1l\x1b[4l\x1b[?6l\x1b[20l\x1b>\x1b[?12l\x1b[?2026l\x1b[?2031l\x1b[=0u\x1b[>4;0m\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l\x1b[?1004l\x1b[?2004l\x1b[3g";

const PANE_RESET_PALETTE: &[u8] = b"\x1b]104\x1b\\\x1b]110\x1b\\\x1b]111\x1b\\\x1b]112\x1b\\";

fn used_visible_rows(terminal: &Terminal<'_, '_>) -> u16 {
    let rows = terminal.rows().unwrap_or(0);
    match capture_terminal(terminal, None, CaptureOptions::default()) {
        Ok(text) if text.is_empty() => 0,
        Ok(text) => text
            .split('\n')
            .collect::<Vec<_>>()
            .iter()
            .rposition(|line| !line.trim_end().is_empty())
            .map_or(0, |index| {
                u16::try_from(index.saturating_add(1))
                    .unwrap_or(rows)
                    .min(rows)
            }),
        Err(_) => rows,
    }
}

fn reset_pane_screen(terminal: &mut Terminal<'_, '_>) -> Result<(), WorkerError> {
    let used = used_visible_rows(terminal);
    let columns = terminal.cols()?;
    let mut program = PANE_RESET_PRELUDE.to_vec();
    let mut column = 8_u16;
    while column < columns {
        write!(program, "\x1b[1;{}H\x1bH", column.saturating_add(1))?;
        column = column.saturating_add(8);
    }
    if used > 0 {
        write!(program, "\x1b[{used}S")?;
    }
    program.extend_from_slice(b"\x1b[2J\x1b[H\x1b7");
    program.extend_from_slice(PANE_RESET_PALETTE);
    terminal.vt_write(&program);
    terminal.scroll_viewport(ScrollViewport::Bottom);
    Ok(())
}

fn capture_viewport(
    viewport: &TerminalViewport,
    options: CaptureOptions,
) -> Result<String, TerminalCaptureError> {
    if options.alternate {
        return Err(TerminalCaptureError::AlternateUnavailable);
    }
    let total = u64::from(viewport.rows);
    if total == 0 {
        return Ok(String::new());
    }
    let visible_end = total.saturating_sub(1);
    let start = resolve_capture_boundary(options.start, 0, visible_end, total);
    let end = resolve_capture_boundary(options.end, 0, visible_end, total);
    if start > end {
        return Ok(String::new());
    }

    let mut output = String::new();
    for row in start..=end {
        if options.escape_sequences {
            capture_viewport_row_vt(
                viewport,
                u16::try_from(row).unwrap_or(u16::MAX),
                options.preserve_trailing,
                &mut output,
            );
        } else {
            capture_viewport_row(
                viewport,
                u16::try_from(row).unwrap_or(u16::MAX),
                options.preserve_trailing,
                &mut output,
            );
        }
        if row < end {
            output.push('\n');
        }
        if output.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
    }
    if options.hyperlinks {
        return Ok(String::new());
    }
    let flags = options
        .line_flags
        .then(|| vec![0; usize::try_from(end - start + 1).unwrap_or(0)]);
    Ok(decorate_capture(
        output,
        None,
        options,
        start,
        0,
        flags.as_deref(),
    ))
}

fn capture_viewport_row(
    viewport: &TerminalViewport,
    row: u16,
    preserve_trailing: bool,
    output: &mut String,
) {
    let start = output.len();
    for cell in viewport.row(row).unwrap_or_default() {
        push_viewport_cell(viewport, *cell, output);
    }
    if !preserve_trailing {
        let trimmed = output[start..].trim_end().len();
        output.truncate(start.saturating_add(trimmed));
    }
}

fn capture_viewport_row_vt(
    viewport: &TerminalViewport,
    row: u16,
    preserve_trailing: bool,
    output: &mut String,
) {
    let cells = viewport.row(row).unwrap_or_default();
    let last = if preserve_trailing {
        cells.len().checked_sub(1)
    } else {
        cells
            .iter()
            .rposition(|cell| viewport_cell_has_non_whitespace(viewport, *cell))
    };
    let Some(last) = last else {
        return;
    };
    let mut active_style = None;
    for cell in &cells[..=last] {
        if matches!(cell.width(), CellWidth::SpacerTail | CellWidth::SpacerHead) {
            continue;
        }
        if active_style != Some(cell.style_id()) {
            if let Some(style) = viewport.style(*cell) {
                mode_revision::push_sgr(output, style);
            }
            active_style = Some(cell.style_id());
        }
        push_viewport_cell(viewport, *cell, output);
    }
    if active_style.is_some() {
        output.push_str("\x1b[0m");
    }
}

fn viewport_cell_has_non_whitespace(viewport: &TerminalViewport, cell: PackedCell) -> bool {
    !matches!(cell.width(), CellWidth::SpacerTail | CellWidth::SpacerHead)
        && cell.glyph() != 0
        && viewport
            .cell_text(cell)
            .chars()
            .any(|character| !character.is_whitespace())
}

fn push_viewport_cell(viewport: &TerminalViewport, cell: PackedCell, output: &mut String) {
    if matches!(cell.width(), CellWidth::SpacerTail | CellWidth::SpacerHead) {
        return;
    }
    if cell.glyph() == 0 {
        output.push(' ');
    } else {
        viewport.push_glyph(cell, output);
    }
}

fn capture_mode_revision(
    mode: &CopyModeState,
    options: CaptureOptions,
) -> Result<String, TerminalCaptureError> {
    capture_revision(
        &mode.revision,
        mode.revision.maximum_offset(),
        options,
        &mut CaptureCarry::default(),
    )
}

fn capture_revision(
    revision: &ModeRevision,
    viewport_offset: u32,
    options: CaptureOptions,
    previous: &mut CaptureCarry,
) -> Result<String, TerminalCaptureError> {
    let total = u64::from(revision.total_rows());
    let visible_start = u64::from(viewport_offset).min(total.saturating_sub(1));
    let visible_end = visible_start
        .saturating_add(u64::from(revision.viewport_rows).saturating_sub(1))
        .min(total.saturating_sub(1));
    let start = resolve_capture_boundary(options.start, visible_start, visible_end, total);
    let end = resolve_capture_boundary(options.end, visible_start, visible_end, total);
    if start > end {
        return Ok(String::new());
    }
    if options.escape_sequences {
        let screen = revision.screen();
        let screen = screen.lock();
        return capture_styled_terminal(
            &*screen,
            CaptureOptions {
                mode: true,
                ..options
            },
            (start, end),
            visible_start,
            revision.columns,
            previous,
            revision.output_rows(),
        );
    }
    let head = u32::try_from(start).unwrap_or(u32::MAX);
    let tail = u32::try_from(end).unwrap_or(u32::MAX);
    let per_row = options.number_lines || options.line_flags;
    let output = revision.capture_rows(
        head,
        tail,
        options.join_wrapped && !per_row,
        options.preserve_trailing,
    );
    if output.len() > MAX_CAPTURE_BYTES {
        return Err(TerminalCaptureError::TooLarge);
    }
    let rows = (per_row && options.join_wrapped).then(|| {
        (head..=tail)
            .map(|row| revision.row(row).wrapped())
            .collect::<Vec<_>>()
    });
    let flags = options.line_flags.then(|| {
        let output_rows = revision.output_rows();
        (head..=tail)
            .map(|row| {
                let meta = revision.row(row);
                (u8::from(revision.row_has_hyperlink(row)) * LINE_FLAG_HYPERLINK)
                    | (u8::from(output_rows.binary_search(&u64::from(row)).is_ok())
                        * LINE_FLAG_OUTPUT)
                    | (u8::from(meta.prompt()) * LINE_FLAG_PROMPT)
                    | (u8::from(meta.wrapped()) * LINE_FLAG_WRAPPED)
            })
            .collect::<Vec<_>>()
    });
    Ok(decorate_capture(
        output,
        rows.as_deref(),
        options,
        start,
        visible_start,
        flags.as_deref(),
    ))
}

fn resolve_capture_boundary(
    boundary: CaptureBoundary,
    visible_start: u64,
    visible_end: u64,
    total: u64,
) -> u64 {
    match boundary {
        CaptureBoundary::HistoryStart => 0,
        CaptureBoundary::VisibleEnd => visible_end,
        CaptureBoundary::Relative(offset) => {
            let resolved = i128::from(visible_start).saturating_add(i128::from(offset));
            u64::try_from(resolved.max(0))
                .unwrap_or(u64::MAX)
                .min(total.saturating_sub(1))
        }
    }
}

fn capture_failure(error: libghostty_vt::Error) -> TerminalCaptureError {
    TerminalCaptureError::Failed(error.to_string())
}

fn semantic_failure(error: &WorkerError) -> TerminalCaptureError {
    TerminalCaptureError::Failed(error.to_string())
}

fn capture_last_command(
    terminal: &Terminal<'_, '_>,
) -> Result<LastCommandCapture, TerminalCaptureError> {
    let total = u32::try_from(terminal.total_rows().map_err(capture_failure)?).unwrap_or(u32::MAX);
    if total == 0 {
        return Err(TerminalCaptureError::NoSemanticMarks);
    }
    let columns = terminal.cols().map_err(capture_failure)?;
    let mut saw_prompt = false;
    let mut block_end = total;
    let mut row = total;
    while row > 0 {
        row -= 1;
        if row_semantic_prompt(terminal, row).map_err(|error| semantic_failure(&error))?
            != RowSemanticPrompt::Prompt
        {
            continue;
        }
        saw_prompt = true;
        let input = input_span(terminal, row, block_end, columns)
            .map_err(|error| semantic_failure(&error))?;
        let Some((command_start, command_end, input_rows_end)) = input else {
            block_end = row;
            continue;
        };
        let command = format_span(terminal, command_start, command_end)?;
        if command.trim().is_empty() {
            block_end = row;
            continue;
        }
        let span = output_span(terminal, input_rows_end, block_end, columns)
            .map_err(|error| semantic_failure(&error))?;
        let (output, truncated_rows) = match span {
            Some((start, end, dropped_rows)) => {
                let output = format_span(terminal, start, end)?;
                let (output, dropped) = clamp_output_bytes(output);
                (output, dropped_rows.saturating_add(dropped))
            }
            None => (String::new(), 0),
        };
        return Ok(LastCommandCapture {
            command,
            output,
            truncated_rows,
            running: false,
        });
    }
    if saw_prompt {
        return Ok(LastCommandCapture::default());
    }
    Err(TerminalCaptureError::NoSemanticMarks)
}

fn row_content_span(
    terminal: &Terminal<'_, '_>,
    row: u32,
    columns: u16,
    content: CellSemanticContent,
) -> Result<Option<(u16, u16)>, WorkerError> {
    let mut span: Option<(u16, u16)> = None;
    for column in 0..columns {
        let cell = terminal
            .grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?
            .cell()?;
        if !cell.has_text()? || cell.semantic_content()? != content {
            continue;
        }
        span = Some(span.map_or((column, column), |(start, _)| (start, column)));
    }
    Ok(span)
}

fn input_span(
    terminal: &Terminal<'_, '_>,
    prompt_row: u32,
    block_end: u32,
    columns: u16,
) -> Result<Option<(PointCoordinate, PointCoordinate, u32)>, WorkerError> {
    let mut span: Option<(PointCoordinate, PointCoordinate)> = None;
    let mut row = prompt_row;
    while row < block_end {
        let Some((first, last)) =
            row_content_span(terminal, row, columns, CellSemanticContent::Input)?
        else {
            break;
        };
        let start = PointCoordinate { x: first, y: row };
        let end = PointCoordinate { x: last, y: row };
        span = Some(span.map_or((start, end), |(existing, _)| (existing, end)));
        row = row.saturating_add(1);
    }
    Ok(span.map(|(start, end)| (start, end, row)))
}

fn output_span(
    terminal: &Terminal<'_, '_>,
    first_row: u32,
    block_end: u32,
    columns: u16,
) -> Result<Option<(PointCoordinate, PointCoordinate, usize)>, WorkerError> {
    let mut last = None;
    let mut row = block_end;
    while row > first_row {
        row -= 1;
        if let Some((_, column)) =
            row_content_span(terminal, row, columns, CellSemanticContent::Output)?
        {
            last = Some(PointCoordinate { x: column, y: row });
            break;
        }
    }
    let Some(last) = last else {
        return Ok(None);
    };
    let mut first = last;
    for row in first_row..=last.y {
        if let Some((column, _)) =
            row_content_span(terminal, row, columns, CellSemanticContent::Output)?
        {
            first = PointCoordinate { x: column, y: row };
            break;
        }
    }
    let limit = u32::try_from(MAX_LAST_COMMAND_LINES).unwrap_or(u32::MAX);
    let rows = last.y.saturating_sub(first.y).saturating_add(1);
    if rows <= limit {
        return Ok(Some((first, last, 0)));
    }
    let dropped = usize::try_from(rows - limit).unwrap_or(usize::MAX);
    let start = PointCoordinate {
        x: 0,
        y: last.y.saturating_sub(limit.saturating_sub(1)),
    };
    Ok(Some((start, last, dropped)))
}

fn clamp_output_bytes(output: String) -> (String, usize) {
    if output.len() <= MAX_LAST_COMMAND_BYTES {
        return (output, 0);
    }
    let mut cut = output.len() - MAX_LAST_COMMAND_BYTES;
    while cut < output.len() && !output.is_char_boundary(cut) {
        cut += 1;
    }
    let start = output
        .match_indices('\n')
        .find(|(offset, _)| *offset >= cut)
        .map_or(cut, |(offset, _)| offset.saturating_add(1));
    let dropped = output[..start].matches('\n').count();
    (output[start..].to_owned(), dropped)
}

fn format_span(
    terminal: &Terminal<'_, '_>,
    start: PointCoordinate,
    end: PointCoordinate,
) -> Result<String, TerminalCaptureError> {
    let start = terminal
        .grid_ref(Point::Screen(start))
        .map_err(capture_failure)?;
    let end = terminal
        .grid_ref(Point::Screen(end))
        .map_err(capture_failure)?;
    let selection = Selection::new(start, end, false);
    let options = FormatterOptions::new()
        .with_format(Format::Plain)
        .with_unwrap(true)
        .with_trim(true)
        .with_selection(&selection);
    let mut formatter = Formatter::new(terminal, options).map_err(capture_failure)?;
    let length = match formatter.format_len() {
        Ok(length) => length,
        Err(libghostty_vt::Error::InvalidValue) => return Ok(String::new()),
        Err(error) => return Err(capture_failure(error)),
    };
    if length > MAX_CAPTURE_BYTES {
        return Err(TerminalCaptureError::TooLarge);
    }
    if length == 0 {
        return Ok(String::new());
    }
    let mut output = vec![0_u8; length];
    let written = formatter.format_buf(&mut output).map_err(capture_failure)?;
    output.truncate(written);
    String::from_utf8(output).map_err(|error| TerminalCaptureError::Failed(error.to_string()))
}

fn apply_copy_mode_action(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    copy_mode: &mut CopyModeSlot,
    unseen_output: &mut u32,
    action: CopyModeAction,
    word_separators: &WordSeparators,
    mode_keys_vi: bool,
) -> Result<ViewActionResult, WorkerError> {
    let Some(mut mode) = copy_mode.take() else {
        return Ok(ViewActionResult::None);
    };

    if mode.search_marks && action.clears_search_marks(mode_keys_vi) {
        mode.search_marks = false;
        mode.search_count = None;
        mode.incremental_origin = None;
    }

    match action {
        ref movement @ (CopyModeAction::Left
        | CopyModeAction::Right
        | CopyModeAction::Up
        | CopyModeAction::Down
        | CopyModeAction::PageUp
        | CopyModeAction::PageDown
        | CopyModeAction::PageDownScrollExit
        | CopyModeAction::HalfPageUp
        | CopyModeAction::HalfPageDown
        | CopyModeAction::HalfPageDownScrollExit
        | CopyModeAction::Top
        | CopyModeAction::Bottom
        | CopyModeAction::TopLine
        | CopyModeAction::MiddleLine
        | CopyModeAction::BottomLine
        | CopyModeAction::StartOfLine
        | CopyModeAction::BackToIndentation
        | CopyModeAction::EndOfLine
        | CopyModeAction::NextWord
        | CopyModeAction::PreviousWord
        | CopyModeAction::NextWordEnd
        | CopyModeAction::NextSpace
        | CopyModeAction::PreviousSpace
        | CopyModeAction::NextSpaceEnd
        | CopyModeAction::NextParagraph
        | CopyModeAction::PreviousParagraph
        | CopyModeAction::NextMatchingBracket
        | CopyModeAction::PreviousMatchingBracket
        | CopyModeAction::NextPrompt { .. }
        | CopyModeAction::PreviousPrompt { .. }
        | CopyModeAction::CursorCentreVertical
        | CopyModeAction::CursorCentreHorizontal
        | CopyModeAction::JumpToMark) => {
            let scroll_exit = match movement {
                CopyModeAction::PageDownScrollExit | CopyModeAction::HalfPageDownScrollExit => true,
                CopyModeAction::PageDown | CopyModeAction::HalfPageDown => mode.scroll_exit,
                _ => false,
            };
            if scroll_exit && copy_mode_scroll_exit_ready(&mode) {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            move_copy_cursor(&mut mode, movement, word_separators, mode_keys_vi);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            if scroll_exit && copy_mode_scroll_exit_ready(&mode) {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::ScrollDownAndCancel => {
            scroll_copy_cursor(&mut mode, false, mode_keys_vi);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            if mode.viewport_offset == mode.revision.maximum_offset() {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::CursorDownAndCancel => {
            *copy_mode = Some(mode);
            cursor_down_and_cancel(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                1,
                word_separators,
                mode_keys_vi,
            )
        }
        scroll_exit_action @ (CopyModeAction::ScrollExitOn
        | CopyModeAction::ScrollExitOff
        | CopyModeAction::ScrollExitToggle) => {
            mode.scroll_exit = match scroll_exit_action {
                CopyModeAction::ScrollExitOn => true,
                CopyModeAction::ScrollExitOff => false,
                _ => !mode.scroll_exit,
            };
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        scroll @ (CopyModeAction::ScrollUp | CopyModeAction::ScrollDown) => {
            let scrolls_toward_bottom = matches!(scroll, CopyModeAction::ScrollDown);
            if scrolls_toward_bottom && mode.scroll_exit && copy_mode_scroll_exit_ready(&mode) {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            scroll_copy_cursor(&mut mode, !scrolls_toward_bottom, mode_keys_vi);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            if scrolls_toward_bottom && mode.scroll_exit && copy_mode_scroll_exit_ready(&mode) {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        placement @ (CopyModeAction::ScrollTop
        | CopyModeAction::ScrollMiddle
        | CopyModeAction::ScrollBottom) => {
            let last = u32::from(mode.revision.viewport_rows.saturating_sub(1));
            let row = match placement {
                CopyModeAction::ScrollTop => 0,
                CopyModeAction::ScrollBottom => last,
                _ => last / 2,
            };
            scroll_copy_view_to_screen_row(&mut mode, row);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::GotoLine {
            line,
            option_absolute,
        } => {
            let absolute = option_absolute && mode.line_numbers != 0;
            goto_copy_line(&mut mode, line, absolute);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::RecentreTopBottom => {
            let row = mode.cursor.y;
            let target = match mode.recentre {
                Some((line, target)) if line == row => target,
                _ => RecentreTarget::Middle,
            };
            let last = u32::from(mode.revision.viewport_rows.saturating_sub(1));
            let (screen_row, next) = match target {
                RecentreTarget::Middle => (last / 2, RecentreTarget::Top),
                RecentreTarget::Top => (0, RecentreTarget::Bottom),
                RecentreTarget::Bottom => (last, RecentreTarget::Middle),
            };
            mode.recentre = Some((row, next));
            mode.viewport_offset = row
                .saturating_sub(screen_row)
                .min(mode.revision.maximum_offset());
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::TogglePosition => {
            mode.hide_position = !mode.hide_position;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        toggle @ (CopyModeAction::RefreshOn
        | CopyModeAction::RefreshOff
        | CopyModeAction::RefreshToggle) => {
            let start = match toggle {
                CopyModeAction::RefreshOn => true,
                CopyModeAction::RefreshOff => false,
                _ => !mode.refresh,
            };
            mode.refresh = start && mode.kind == FrozenModeKind::Copy && !mode.sourced;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::LineNumbersOn { option_off } => {
            mode.line_numbers = if option_off { 2 } else { 1 };
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::LineNumbersOff => {
            mode.line_numbers = 0;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::LineNumbersToggle { option_off } => {
            let active = mode.line_numbers == 2 || (mode.line_numbers == 1 && !option_off);
            mode.line_numbers = match (active, option_off) {
                (true, _) => 0,
                (false, true) => 2,
                (false, false) => 1,
            };
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::RefreshNow => {
            if mode.kind != FrozenModeKind::Copy || mode.sourced {
                *copy_mode = Some(mode);
                return Ok(ViewActionResult::None);
            }
            refresh_copy_revision(terminal, selection, &mut mode, mode_keys_vi)?;
            *unseen_output = 0;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::RefreshRevision => {
            if !mode.refresh
                || mode.kind != FrozenModeKind::Copy
                || mode.selection.is_some()
                || mode.selecting
                || *unseen_output == 0
            {
                *copy_mode = Some(mode);
                return Ok(ViewActionResult::None);
            }
            refresh_copy_revision(terminal, selection, &mut mode, mode_keys_vi)?;
            *unseen_output = 0;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::StartSelection => {
            mode.selection = Some(ModeSelection {
                anchor: mode.cursor,
                focus: mode.cursor,
                mode: SelectionMode::Cell,
                rectangle: mode.rectangle,
            });
            mode.selection_mode = CopySelectionMode::Char;
            mode.selection_origin = mode.cursor;
            mode.selecting = true;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::SelectWord => {
            let (anchor, mut focus) =
                mode_word_bounds(&mode.revision, mode.cursor, word_separators);
            if !mode_keys_vi {
                focus.x = focus
                    .x
                    .saturating_add(1)
                    .min(mode.revision.columns.saturating_sub(1));
            }
            mode.selection = Some(ModeSelection {
                anchor,
                focus,
                mode: SelectionMode::Word,
                rectangle: false,
            });
            mode.selection_mode = CopySelectionMode::Word;
            mode.selection_origin = mode.cursor;
            mode.cursor = focus;
            mode.selecting = true;
            mode.rectangle = false;
            reveal_copy_cursor(&mut mode);
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::SelectLine => {
            select_copy_mode_lines(&mut mode, 1, mode_keys_vi);
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::ClearSelection => {
            *selection = None;
            terminal.set_selection(None)?;
            mode.selection = None;
            mode.selection_mode = CopySelectionMode::Char;
            mode.selecting = false;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::ClearSelectionOrCancel if mode.selection.is_some() || mode.selecting => {
            *selection = None;
            terminal.set_selection(None)?;
            mode.selection = None;
            mode.selection_mode = CopySelectionMode::Char;
            mode.selecting = false;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::ToggleRectangle => {
            mode.rectangle = !mode.rectangle;
            if let Some(selection) = mode.selection.as_mut() {
                selection.rectangle = mode.rectangle;
                mode.selecting = true;
            }
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        rectangle_action @ (CopyModeAction::RectangleOn | CopyModeAction::RectangleOff) => {
            mode.rectangle = matches!(rectangle_action, CopyModeAction::RectangleOn);
            if let Some(selection) = mode.selection.as_mut() {
                selection.rectangle = mode.rectangle;
                mode.selecting = true;
            }
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::OtherEnd => {
            mode.selection_mode = CopySelectionMode::Char;
            if let Some(selection) = mode.selection.as_mut() {
                std::mem::swap(&mut selection.anchor, &mut selection.focus);
                mode.cursor = selection.focus;
                mode.selection_origin = mode.cursor;
                mode.selecting = true;
                reveal_copy_cursor(&mut mode);
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::SetMark => {
            mode.mark = Some(mode.cursor);
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::Jump(jump) => {
            let latched_vi = mode.mode_keys_vi_at_entry;
            apply_copy_jump(&mut mode, &jump, latched_vi);
            mode.last_jump = Some(jump);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::RepeatJump { reverse } => {
            if let Some(mut jump) = mode.last_jump.clone() {
                if reverse {
                    jump.direction = match jump.direction {
                        CopyJumpDirection::Forward => CopyJumpDirection::Backward,
                        CopyJumpDirection::Backward => CopyJumpDirection::Forward,
                    };
                }
                let latched_vi = mode.mode_keys_vi_at_entry;
                apply_copy_jump(&mut mode, &jump, latched_vi);
                if mode.selecting {
                    update_copy_selection(&mut mode, Some(word_separators));
                }
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::CopySelection(copy) => {
            let copy = *copy;
            let text = mode.selection.map_or_else(String::new, |selection| {
                mode.revision.format_selection(selection, mode_keys_vi)
            });
            let view_changed = copy.clear_selection || copy.cancel;
            if copy.cancel {
                *selection = None;
                terminal.set_selection(None)?;
                terminal.scroll_viewport(ScrollViewport::Bottom);
                *unseen_output = 0;
            } else {
                if copy.clear_selection {
                    mode.selection = None;
                    mode.selecting = false;
                }
                *copy_mode = Some(mode);
            }
            Ok(ViewActionResult::Copy(Box::new(TerminalCopyReady {
                request_id: copy.request_id,
                clipboard: copy.clipboard.then_some(ClipboardTarget::Clipboard),
                buffer: copy.buffer,
                pipe: copy.pipe,
                text,
                view_changed,
            })))
        }
        CopyModeAction::CopyEndOfLine(copy) => {
            select_copy_mode_to_line_end(&mut mode, 1, mode_keys_vi);
            *copy_mode = Some(mode);
            apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                CopyModeAction::CopySelection(copy),
                word_separators,
                mode_keys_vi,
            )
        }
        CopyModeAction::CopyLine(copy) => {
            select_copy_mode_whole_line(&mut mode, 1, mode_keys_vi);
            *copy_mode = Some(mode);
            apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                CopyModeAction::CopySelection(copy),
                word_separators,
                mode_keys_vi,
            )
        }
        CopyModeAction::SelectionMode(unit) => {
            mode.selection_mode = unit;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::MouseCursor(cell) => {
            mode.cursor = mode_pointer_point(&mode, cell);
            reveal_copy_cursor(&mut mode);
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::StopSelection => {
            mode.selection_mode = CopySelectionMode::Char;
            mode.selecting = false;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::SearchAgain { .. }
        | CopyModeAction::SearchCursorWord { .. }
        | CopyModeAction::Search(_) => {
            *copy_mode = Some(mode);
            Ok(ViewActionResult::None)
        }
        CopyModeAction::Cancel | CopyModeAction::ClearSelectionOrCancel => {
            cancel_copy_mode(terminal, selection, unseen_output)
        }
    }
}

fn select_copy_mode_to_line_end(mode: &mut CopyModeState, count: u32, mode_keys_vi: bool) {
    if count == 0 {
        return;
    }
    let focus_y = mode
        .cursor
        .y
        .saturating_add(count - 1)
        .min(mode.revision.total_rows().saturating_sub(1));
    mode.selection = Some(ModeSelection {
        anchor: mode.cursor,
        focus: PointCoordinate {
            x: copy_cursor_limit(&mode.revision, focus_y, mode_keys_vi, false),
            y: focus_y,
        },
        mode: SelectionMode::Cell,
        rectangle: false,
    });
    mode.selecting = true;
}

fn apply_counted_copy_mode_action(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    copy_mode: &mut CopyModeSlot,
    unseen_output: &mut u32,
    action: CopyModeAction,
    count: u32,
    word_separators: &WordSeparators,
    mode_keys_vi: bool,
) -> Result<ViewActionResult, WorkerError> {
    if count == 0 {
        return Ok(ViewActionResult::None);
    }
    match action.count_policy() {
        CopyModeCountPolicy::Repeat => {
            if copy_mode.is_none() {
                return Ok(ViewActionResult::None);
            }
            for _ in 0..count {
                apply_copy_mode_action(
                    terminal,
                    selection,
                    copy_mode,
                    unseen_output,
                    action.clone(),
                    word_separators,
                    mode_keys_vi,
                )?;
                if copy_mode.is_none() {
                    break;
                }
            }
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeCountPolicy::OtherEnd if count.is_multiple_of(2) => Ok(ViewActionResult::Snapshot),
        CopyModeCountPolicy::OtherEnd | CopyModeCountPolicy::Once => apply_copy_mode_action(
            terminal,
            selection,
            copy_mode,
            unseen_output,
            action,
            word_separators,
            mode_keys_vi,
        ),
        CopyModeCountPolicy::CursorDownAndCancel => cursor_down_and_cancel(
            terminal,
            selection,
            copy_mode,
            unseen_output,
            count,
            word_separators,
            mode_keys_vi,
        ),
        CopyModeCountPolicy::SelectLine => {
            let Some(mode) = copy_mode.as_mut() else {
                return Ok(ViewActionResult::None);
            };
            select_copy_mode_lines(mode, count, mode_keys_vi);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeCountPolicy::CopyEndOfLine => {
            let CopyModeAction::CopyEndOfLine(copy) = action else {
                unreachable!("copy-end policy is only assigned to copy-end-of-line")
            };
            let Some(mut mode) = copy_mode.take() else {
                return Ok(ViewActionResult::None);
            };
            select_copy_mode_to_line_end(&mut mode, count, mode_keys_vi);
            *copy_mode = Some(mode);
            apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                CopyModeAction::CopySelection(copy),
                word_separators,
                mode_keys_vi,
            )
        }
        CopyModeCountPolicy::CopyLine => {
            let CopyModeAction::CopyLine(copy) = action else {
                unreachable!("copy-line policy is only assigned to copy-line")
            };
            let Some(mut mode) = copy_mode.take() else {
                return Ok(ViewActionResult::None);
            };
            select_copy_mode_whole_line(&mut mode, count, mode_keys_vi);
            *copy_mode = Some(mode);
            apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                CopyModeAction::CopySelection(copy),
                word_separators,
                mode_keys_vi,
            )
        }
    }
}

fn select_copy_mode_whole_line(mode: &mut CopyModeState, count: u32, mode_keys_vi: bool) {
    if count == 0 {
        return;
    }
    let (anchor, _) = mode_logical_line_bounds(&mode.revision, mode.cursor.y);
    let row = mode
        .cursor
        .y
        .saturating_add(count.saturating_sub(1))
        .min(mode.revision.total_rows().saturating_sub(1));
    let (_, end) = mode_logical_line_bounds(&mode.revision, row);
    let focus = PointCoordinate {
        x: copy_cursor_limit(&mode.revision, end.y, mode_keys_vi, false),
        y: end.y,
    };
    mode.selection_mode = CopySelectionMode::Char;
    mode.selection = Some(ModeSelection {
        anchor,
        focus,
        mode: SelectionMode::Cell,
        rectangle: false,
    });
    mode.selecting = true;
}

fn select_copy_mode_lines(mode: &mut CopyModeState, count: u32, mode_keys_vi: bool) {
    if count == 0 {
        return;
    }
    let origin = mode.cursor;
    let (anchor, mut focus) = mode_logical_line_bounds(&mode.revision, mode.cursor.y);
    for _ in 1..count {
        let next = focus.y.saturating_add(1);
        if next >= mode.revision.total_rows() {
            break;
        }
        let (_, next_focus) = mode_logical_line_bounds(&mode.revision, next);
        if next_focus.y == focus.y {
            break;
        }
        focus = next_focus;
    }
    focus.x = copy_cursor_limit(&mode.revision, focus.y, mode_keys_vi, false);
    mode.cursor = PointCoordinate {
        x: revision_copy_line_end(&mode.revision, focus.y),
        y: focus.y,
    };
    mode.selection = Some(ModeSelection {
        anchor,
        focus,
        mode: SelectionMode::Line,
        rectangle: false,
    });
    mode.selection_mode = CopySelectionMode::Line;
    mode.selection_origin = origin;
    mode.selecting = true;
    mode.rectangle = false;
    reveal_copy_cursor(mode);
}

fn goto_copy_line(mode: &mut CopyModeState, line: Option<i32>, absolute: bool) {
    let Some(line) = line else {
        return;
    };
    let maximum = mode.revision.maximum_offset();
    let offset = if absolute {
        u32::try_from(line.max(1) - 1).unwrap_or(0).min(maximum)
    } else {
        maximum.saturating_sub(u32::try_from(line).unwrap_or(maximum).min(maximum))
    };
    let delta = i64::from(offset) - i64::from(mode.viewport_offset);
    mode.viewport_offset = offset;
    let last = i64::from(mode.revision.total_rows().saturating_sub(1));
    let row = (i64::from(mode.cursor.y) + delta).clamp(0, last);
    mode.cursor.y = u32::try_from(row).unwrap_or(0);
    mode.cursor = mode.revision.clamp_point(mode.cursor);
}

fn scroll_copy_view_to_screen_row(mode: &mut CopyModeState, row: u32) {
    let Some(target) = mode.cursor.y.checked_sub(row) else {
        return;
    };
    if target <= mode.revision.maximum_offset() {
        mode.viewport_offset = target;
    }
}

fn cursor_down_and_cancel(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    copy_mode: &mut CopyModeSlot,
    unseen_output: &mut u32,
    count: u32,
    word_separators: &WordSeparators,
    mode_keys_vi: bool,
) -> Result<ViewActionResult, WorkerError> {
    let Some(start) = copy_mode.as_deref().map(|mode| mode.cursor.y) else {
        return Ok(ViewActionResult::None);
    };
    for _ in 0..count {
        apply_copy_mode_action(
            terminal,
            selection,
            copy_mode,
            unseen_output,
            CopyModeAction::Down,
            word_separators,
            mode_keys_vi,
        )?;
    }
    let Some(mode) = copy_mode.as_deref() else {
        return Ok(ViewActionResult::Snapshot);
    };
    if mode.cursor.y == start && mode.viewport_offset == mode.revision.maximum_offset() {
        *copy_mode = None;
        return cancel_copy_mode(terminal, selection, unseen_output);
    }
    Ok(ViewActionResult::Snapshot)
}

fn copy_mode_scroll_exit_ready(mode: &CopyModeState) -> bool {
    mode.viewport_offset == mode.revision.maximum_offset()
        && mode.selection.is_none()
        && !mode.selecting
}

fn cancel_copy_mode(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    unseen_output: &mut u32,
) -> Result<ViewActionResult, WorkerError> {
    *selection = None;
    terminal.set_selection(None)?;
    terminal.scroll_viewport(ScrollViewport::Bottom);
    *unseen_output = 0;
    Ok(ViewActionResult::Snapshot)
}

fn reader_jump_forward(
    revision: &ModeRevision,
    mut point: PointCoordinate,
    target: &str,
) -> Option<PointCoordinate> {
    let last = revision.total_rows().saturating_sub(1);
    loop {
        let length = revision_line_length(revision, point.y);
        while point.x < length {
            if revision.cell_matches_text(point, target) {
                return Some(point);
            }
            point.x += 1;
        }
        if point.y >= last || !revision.row(point.y).wrapped() {
            return None;
        }
        point.y += 1;
        point.x = 0;
    }
}

fn reader_jump_backward(
    revision: &ModeRevision,
    mut point: PointCoordinate,
    target: &str,
) -> Option<PointCoordinate> {
    let mut edge = point.x.saturating_add(1);
    loop {
        let mut column = edge;
        while column > 0 {
            column -= 1;
            let candidate = PointCoordinate {
                x: column,
                y: point.y,
            };
            if revision.cell_matches_text(candidate, target) {
                return Some(candidate);
            }
        }
        if point.y == 0 || !revision.row(point.y - 1).wrapped() {
            return None;
        }
        point.y -= 1;
        edge = revision_line_length(revision, point.y);
    }
}

fn apply_copy_jump(mode: &mut CopyModeState, jump: &CopyJump, vi: bool) {
    let start = mode.cursor;
    let found = match (jump.direction, jump.to) {
        (CopyJumpDirection::Forward, false) => reader_jump_forward(
            &mode.revision,
            PointCoordinate {
                x: start.x.saturating_add(1),
                y: start.y,
            },
            &jump.target,
        ),
        (CopyJumpDirection::Forward, true) => reader_jump_forward(
            &mode.revision,
            PointCoordinate {
                x: start.x.saturating_add(2),
                y: start.y,
            },
            &jump.target,
        )
        .map(|mut point| {
            reader_cursor_left(&mode.revision, &mut point, true);
            point
        }),
        (CopyJumpDirection::Backward, false) => {
            let mut point = start;
            reader_cursor_left(&mode.revision, &mut point, false);
            reader_jump_backward(&mode.revision, point, &jump.target)
        }
        (CopyJumpDirection::Backward, true) => {
            let mut point = start;
            reader_cursor_left(&mode.revision, &mut point, false);
            reader_cursor_left(&mode.revision, &mut point, false);
            reader_jump_backward(&mode.revision, point, &jump.target).map(|mut point| {
                reader_cursor_right(&mode.revision, &mut point, true, false, !vi);
                point
            })
        }
    };
    if let Some(point) = found {
        mode.cursor = point;
        reveal_copy_cursor(mode);
    }
}
fn move_copy_cursor(
    mode: &mut CopyModeState,
    action: &CopyModeAction,
    word_separators: &WordSeparators,
    vi: bool,
) {
    let mut point = mode.cursor;
    let total = mode.revision.total_rows();
    let page = u32::from(mode.revision.viewport_rows);
    match action {
        CopyModeAction::Left => {
            reader_cursor_left(&mode.revision, &mut point, true);
        }
        CopyModeAction::Right => {
            let all = mode.selection.is_some() && mode.rectangle;
            reader_cursor_right(&mode.revision, &mut point, true, all, !vi);
        }
        CopyModeAction::Up => move_copy_cursor_row(mode, &mut point, false, vi),
        CopyModeAction::Down => move_copy_cursor_row(mode, &mut point, true, vi),
        CopyModeAction::PageUp => return page_copy_cursor(mode, true, false, vi),
        CopyModeAction::PageDown | CopyModeAction::PageDownScrollExit => {
            return page_copy_cursor(mode, false, false, vi);
        }
        CopyModeAction::HalfPageUp => return page_copy_cursor(mode, true, true, vi),
        CopyModeAction::HalfPageDown | CopyModeAction::HalfPageDownScrollExit => {
            return page_copy_cursor(mode, false, true, vi);
        }
        CopyModeAction::Top => {
            point.x = 0;
            point.y = 0;
        }
        CopyModeAction::Bottom => point.y = total - 1,
        CopyModeAction::CursorCentreVertical => {
            let middle = u32::from(mode.revision.viewport_rows) / 2;
            point.y = mode
                .viewport_offset
                .saturating_add(middle)
                .min(total.saturating_sub(1));
        }
        CopyModeAction::CursorCentreHorizontal => {
            point.x = (mode.revision.columns / 2).min(mode.revision.columns.saturating_sub(1));
        }
        CopyModeAction::TopLine => {
            point.x = 0;
            point.y = mode.viewport_offset;
        }
        CopyModeAction::MiddleLine => {
            point.x = 0;
            point.y = mode
                .viewport_offset
                .saturating_add(page.saturating_sub(1) / 2)
                .min(total - 1);
        }
        CopyModeAction::BottomLine => {
            point.x = 0;
            point.y = mode
                .viewport_offset
                .saturating_add(page.saturating_sub(1))
                .min(total - 1);
        }
        CopyModeAction::StartOfLine => {
            point.y = reader_logical_line_start(&mode.revision, point.y);
            point.x = 0;
        }
        CopyModeAction::BackToIndentation => {
            point.y = reader_logical_line_start(&mode.revision, point.y);
            point.x = revision_first_non_whitespace(&mode.revision, point.y);
        }
        CopyModeAction::EndOfLine => {
            point.y = reader_logical_line_end(&mode.revision, point.y);
            point.x = revision_line_length(&mode.revision, point.y);
        }
        CopyModeAction::NextWordEnd => {
            point = move_revision_word_end(&mode.revision, point, Some(word_separators), vi);
        }
        CopyModeAction::NextSpaceEnd => {
            point = move_revision_word_end(&mode.revision, point, None, vi);
        }
        CopyModeAction::NextWord
        | CopyModeAction::PreviousWord
        | CopyModeAction::NextSpace
        | CopyModeAction::PreviousSpace => {
            point = move_revision_word(&mode.revision, point, action, word_separators);
        }
        CopyModeAction::PreviousMatchingBracket => {
            let latched_vi = mode.mode_keys_vi_at_entry;
            if let Some(target) = revision_matching_bracket(&mode.revision, point, true, latched_vi)
            {
                point = target;
            } else if !latched_vi {
                point = move_revision_word(
                    &mode.revision,
                    point,
                    &CopyModeAction::PreviousWord,
                    &WordSeparators::new(CLOSING_BRACKETS),
                );
            }
        }
        CopyModeAction::NextMatchingBracket => {
            let latched_vi = mode.mode_keys_vi_at_entry;
            if let Some(target) =
                revision_matching_bracket(&mode.revision, point, false, latched_vi)
            {
                point = target;
            } else if !latched_vi {
                point = move_revision_word_end(
                    &mode.revision,
                    point,
                    Some(&WordSeparators::new(OPENING_BRACKETS)),
                    latched_vi,
                );
            }
        }
        CopyModeAction::NextParagraph => {
            point.y = revision_paragraph_target(&mode.revision, point.y, 1);
        }
        CopyModeAction::PreviousParagraph => {
            point.y = revision_paragraph_target(&mode.revision, point.y, -1);
        }
        CopyModeAction::NextPrompt { output } => {
            if let Some(target) =
                revision_semantic_prompt_target(&mode.revision, point.y, 1, *output)
            {
                point = target;
            }
        }
        CopyModeAction::PreviousPrompt { output } => {
            if let Some(target) =
                revision_semantic_prompt_target(&mode.revision, point.y, -1, *output)
            {
                point = target;
            }
        }
        CopyModeAction::JumpToMark => {
            if let Some(mark) = mode.mark {
                point = mark;
            }
        }
        _ => return,
    }
    place_copy_cursor(mode, point, vi);
    reveal_copy_cursor(mode);
}

fn scroll_copy_cursor(mode: &mut CopyModeState, up: bool, vi: bool) {
    let previous = mode.viewport_offset;
    let page = u32::from(mode.revision.viewport_rows.max(1));
    if up {
        mode.viewport_offset = previous.saturating_sub(1);
        if mode.viewport_offset < previous
            && (!vi || mode.cursor.y >= previous.saturating_add(page.saturating_sub(1)))
        {
            mode.cursor.y = mode.cursor.y.saturating_sub(1);
        }
    } else {
        mode.viewport_offset = previous
            .saturating_add(1)
            .min(mode.revision.maximum_offset());
        if mode.viewport_offset > previous && (!vi || mode.cursor.y <= previous) {
            mode.cursor.y = mode
                .cursor
                .y
                .saturating_add(1)
                .min(mode.revision.total_rows().saturating_sub(1));
        }
    }
}

fn page_copy_cursor(mode: &mut CopyModeState, up: bool, half: bool, vi: bool) {
    let rows = u32::from(mode.revision.viewport_rows);
    let length = revision_line_length(&mode.revision, mode.cursor.y);
    if mode.cursor.x != length {
        mode.last_cx = mode.cursor.x;
        mode.last_sx = length;
    }
    let step = match (rows > 2, half) {
        (false, _) => 1,
        (true, true) => rows / 2,
        (true, false) => rows - 2,
    };
    let history = mode.revision.maximum_offset();
    let mut from_bottom = history.saturating_sub(mode.viewport_offset);
    let mut row = mode.cursor.y.saturating_sub(mode.viewport_offset);
    if up {
        if from_bottom.saturating_add(step) > history {
            from_bottom = history;
            row = row.saturating_sub(step);
        } else {
            from_bottom += step;
        }
    } else if from_bottom < step {
        from_bottom = 0;
        row = row.saturating_add(step).min(rows.saturating_sub(1));
    } else {
        from_bottom -= step;
    }
    mode.viewport_offset = history - from_bottom;
    let y = mode
        .viewport_offset
        .saturating_add(row)
        .min(mode.revision.total_rows().saturating_sub(1));
    let x = mode.last_cx;
    if !(mode.selection.is_some() && mode.rectangle) {
        let end = revision_line_length(&mode.revision, y);
        if (x >= mode.last_sx && x != end) || x > end {
            place_copy_cursor(mode, PointCoordinate { x: end, y }, vi);
            return;
        }
    }
    mode.cursor = PointCoordinate { x, y };
}

fn scroll_copy_view_to_cursor(mode: &mut CopyModeState) {
    let rows = u32::from(mode.revision.viewport_rows);
    let history = mode.revision.maximum_offset();
    let y = mode.cursor.y;
    if y >= mode.viewport_offset && y < mode.viewport_offset.saturating_add(rows) {
        return;
    }
    let gap = rows / 4;
    mode.viewport_offset = if y < rows {
        0
    } else if y > history.saturating_add(rows).saturating_sub(gap) {
        history
    } else {
        y.saturating_add(gap).saturating_sub(rows).min(history)
    };
}

fn reveal_copy_cursor(mode: &mut CopyModeState) {
    let page = u32::from(mode.revision.viewport_rows);
    if mode.cursor.y < mode.viewport_offset {
        mode.viewport_offset = mode.cursor.y;
    } else if mode.cursor.y >= mode.viewport_offset.saturating_add(page) {
        mode.viewport_offset = mode.cursor.y.saturating_sub(page.saturating_sub(1));
    }
    mode.viewport_offset = mode.viewport_offset.min(mode.revision.maximum_offset());
}

fn revision_copy_line_end(revision: &ModeRevision, row: u32) -> u16 {
    for column in (0..revision.columns).rev() {
        if !revision_cell_is_whitespace(revision, PointCoordinate { x: column, y: row }) {
            return column;
        }
    }
    0
}

fn revision_line_length(revision: &ModeRevision, row: u32) -> u16 {
    for column in (0..revision.columns).rev() {
        if !revision_cell_is_whitespace(revision, PointCoordinate { x: column, y: row }) {
            return column.saturating_add(1);
        }
    }
    0
}

fn copy_cursor_limit(revision: &ModeRevision, row: u32, vi: bool, allow_onemore: bool) -> u16 {
    let length = revision_line_length(revision, row);
    if allow_onemore || !vi {
        length
    } else {
        length.saturating_sub(1)
    }
}

fn copy_cursor_clamp(mode: &CopyModeState, point: PointCoordinate, vi: bool) -> PointCoordinate {
    let y = point.y.min(mode.revision.total_rows().saturating_sub(1));
    if mode.rectangle {
        return PointCoordinate {
            x: point.x.min(mode.revision.columns),
            y,
        };
    }
    PointCoordinate {
        x: point.x.min(copy_cursor_limit(&mode.revision, y, vi, false)),
        y,
    }
}

fn place_copy_cursor(mode: &mut CopyModeState, point: PointCoordinate, vi: bool) {
    mode.cursor = copy_cursor_clamp(mode, point, vi);
}

fn revision_cell_is_padding(revision: &ModeRevision, point: PointCoordinate) -> bool {
    matches!(
        revision.cell(point).width(),
        CellWidth::SpacerTail | CellWidth::SpacerHead
    )
}

fn reader_cursor_right(
    revision: &ModeRevision,
    point: &mut PointCoordinate,
    wrap: bool,
    all: bool,
    onemore: bool,
) {
    let limit = if all {
        revision.columns
    } else {
        copy_cursor_limit(revision, point.y, !onemore, false)
    };
    if wrap && point.x >= limit && point.y.saturating_add(1) < revision.total_rows() {
        point.x = 0;
        point.y += 1;
    } else if point.x < limit {
        point.x += 1;
        while point.x < limit && revision_cell_is_padding(revision, *point) {
            point.x += 1;
        }
    }
}

fn reader_cursor_left(revision: &ModeRevision, point: &mut PointCoordinate, wrap: bool) {
    while point.x > 0 && revision_cell_is_padding(revision, *point) {
        point.x -= 1;
    }
    if point.x == 0 && point.y > 0 && (wrap || revision.row(point.y - 1).wrapped()) {
        point.y -= 1;
        while point.x > 0 && revision_cell_is_padding(revision, *point) {
            point.x -= 1;
        }
        point.x = revision_line_length(revision, point.y);
    } else if point.x > 0 {
        point.x -= 1;
    }
}

fn reader_logical_line_end(revision: &ModeRevision, row: u32) -> u32 {
    let mut row = row;
    let last = revision.total_rows().saturating_sub(1);
    while row < last && revision.row(row).wrapped() {
        row += 1;
    }
    row
}

fn reader_logical_line_start(revision: &ModeRevision, row: u32) -> u32 {
    let mut row = row;
    while row > 0 && revision.row(row - 1).wrapped() {
        row -= 1;
    }
    row
}

fn reader_handle_wrap(
    revision: &ModeRevision,
    point: &mut PointCoordinate,
    bound: &mut u16,
    last_row: u32,
) -> bool {
    while point.x > *bound {
        if point.y >= last_row {
            return false;
        }
        point.x = 0;
        point.y += 1;
        *bound = reader_row_bound(revision, point.y);
    }
    true
}

fn reader_row_bound(revision: &ModeRevision, row: u32) -> u16 {
    if revision.row(row).wrapped() {
        revision.columns.saturating_sub(1)
    } else {
        revision_line_length(revision, row)
    }
}

fn reader_next_word_end(
    revision: &ModeRevision,
    point: &mut PointCoordinate,
    separators: Option<&WordSeparators>,
) {
    let last_row = revision.total_rows().saturating_sub(1);
    let mut bound = reader_row_bound(revision, point.y);
    let is_blank = |point: PointCoordinate| revision_cell_is_whitespace(revision, point);
    let is_separator = |point: PointCoordinate| {
        separators.is_some_and(|separators| {
            revision
                .first_char(point)
                .is_some_and(|character| separators.contains_separator(character))
        })
    };
    while reader_handle_wrap(revision, point, &mut bound, last_row) {
        if is_blank(*point) {
            let Some(next) = point.x.checked_add(1) else {
                return;
            };
            point.x = next;
        } else if is_separator(*point) {
            loop {
                let Some(next) = point.x.checked_add(1) else {
                    return;
                };
                point.x = next;
                if !(reader_handle_wrap(revision, point, &mut bound, last_row)
                    && is_separator(*point)
                    && !is_blank(*point))
                {
                    return;
                }
            }
        } else {
            loop {
                let Some(next) = point.x.checked_add(1) else {
                    return;
                };
                point.x = next;
                if !(reader_handle_wrap(revision, point, &mut bound, last_row)
                    && !(is_blank(*point) || is_separator(*point)))
                {
                    return;
                }
            }
        }
    }
}

fn move_revision_word_end(
    revision: &ModeRevision,
    mut point: PointCoordinate,
    separators: Option<&WordSeparators>,
    vi: bool,
) -> PointCoordinate {
    if vi {
        if !revision_cell_is_whitespace(revision, point) {
            reader_cursor_right(revision, &mut point, false, false, false);
        }
        reader_next_word_end(revision, &mut point, separators);
        reader_cursor_left(revision, &mut point, true);
    } else {
        reader_next_word_end(revision, &mut point, separators);
    }
    point
}

fn move_copy_cursor_row(
    mode: &mut CopyModeState,
    point: &mut PointCoordinate,
    down: bool,
    vi: bool,
) {
    let rectangle_selection = mode.selection.is_some() && mode.rectangle;
    let length = revision_line_length(&mode.revision, point.y);
    if !rectangle_selection && point.x != length {
        mode.last_cx = point.x;
        mode.last_sx = length;
    }
    let total = mode.revision.total_rows();
    point.y = if down {
        point.y.saturating_add(1).min(total.saturating_sub(1))
    } else {
        point.y.saturating_sub(1)
    };
    if rectangle_selection {
        return;
    }
    point.x = mode.last_cx;
    *point = copy_cursor_clamp(mode, *point, vi);
    let end = revision_line_length(&mode.revision, point.y);
    if (point.x >= mode.last_sx && point.x != end) || point.x > end {
        point.x = end;
    }
}

const CLOSING_BRACKETS: &str = "}])";
const OPENING_BRACKETS: &str = "{[(";

fn revision_matching_bracket(
    revision: &ModeRevision,
    point: PointCoordinate,
    backward: bool,
    vi: bool,
) -> Option<PointCoordinate> {
    let columns = u64::from(revision.columns);
    let total = columns.saturating_mul(u64::from(revision.total_rows()));
    let to_point = |index: u64| PointCoordinate {
        x: u16::try_from(index % columns).unwrap_or(0),
        y: u32::try_from(index / columns).unwrap_or(u32::MAX),
    };
    let char_at = |index: u64| revision.first_char(to_point(index));
    let origin = u64::from(point.y)
        .saturating_mul(columns)
        .saturating_add(u64::from(point.x))
        .min(total.saturating_sub(1));

    let mut bracket = origin;
    let is_target = |character: Option<char>| {
        if backward {
            matches!(character, Some(')' | ']' | '}'))
        } else if vi {
            matches!(character, Some('(' | ')' | '[' | ']' | '{' | '}'))
        } else {
            matches!(character, Some('(' | '[' | '{'))
        }
    };
    if !vi {
        if is_target(char_at(origin)) {
            bracket = origin;
        } else {
            let neighbour = if backward {
                (point.x > 0).then(|| origin.saturating_sub(1))
            } else {
                (point.x.saturating_add(1) < revision.columns).then(|| origin.saturating_add(1))
            };
            let neighbour = neighbour.filter(|index| is_target(char_at(*index)))?;
            bracket = neighbour;
        }
    }
    while !is_target(char_at(bracket)) {
        let at = to_point(bracket);
        let reaches_further = if backward {
            at.x > 0 || (at.y > 0 && revision.row(at.y).continuation())
        } else {
            at.x.saturating_add(1) < revision.columns
                || (revision.row(at.y).wrapped() && at.y.saturating_add(1) < revision.total_rows())
        };
        if !reaches_further {
            return None;
        }
        bracket = if backward {
            bracket.saturating_sub(1)
        } else {
            bracket.saturating_add(1)
        };
    }

    let found = char_at(bracket)?;
    let (open, close, forward) = match found {
        '(' => ('(', ')', true),
        '[' => ('[', ']', true),
        '{' => ('{', '}', true),
        ')' => ('(', ')', false),
        ']' => ('[', ']', false),
        '}' => ('{', '}', false),
        _ => return None,
    };
    let mut depth = 1_u32;
    if forward {
        for index in bracket.saturating_add(1)..total {
            match char_at(index) {
                Some(character) if character == open => depth = depth.saturating_add(1),
                Some(character) if character == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(to_point(index));
                    }
                }
                _ => {}
            }
        }
    } else {
        for index in (0..bracket).rev() {
            match char_at(index) {
                Some(character) if character == close => depth = depth.saturating_add(1),
                Some(character) if character == open => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(to_point(index));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn revision_first_non_whitespace(revision: &ModeRevision, row: u32) -> u16 {
    (0..revision.columns)
        .find(|column| {
            !revision_cell_is_whitespace(revision, PointCoordinate { x: *column, y: row })
        })
        .unwrap_or(0)
}

fn revision_row_is_blank(revision: &ModeRevision, row: u32) -> bool {
    (0..revision.columns)
        .all(|column| revision_cell_is_whitespace(revision, PointCoordinate { x: column, y: row }))
}

fn revision_cell_is_whitespace(revision: &ModeRevision, point: PointCoordinate) -> bool {
    revision.first_char(point).is_none_or(char::is_whitespace)
}

fn revision_paragraph_target(revision: &ModeRevision, row: u32, direction: i8) -> u32 {
    let total = revision.total_rows();
    if direction.is_negative() {
        let mut target = row.saturating_sub(1);
        while target > 0 && revision_row_is_blank(revision, target) {
            target -= 1;
        }
        while target > 0 && !revision_row_is_blank(revision, target - 1) {
            target -= 1;
        }
        target
    } else {
        let mut target = row.saturating_add(1).min(total - 1);
        while target + 1 < total && !revision_row_is_blank(revision, target) {
            target += 1;
        }
        while target + 1 < total && revision_row_is_blank(revision, target) {
            target += 1;
        }
        target
    }
}

fn mode_logical_line_bounds(
    revision: &ModeRevision,
    row: u32,
) -> (PointCoordinate, PointCoordinate) {
    let mut start = row;
    while start > 0 && revision.row(start).continuation() {
        start -= 1;
    }
    let mut end = row;
    while end + 1 < revision.total_rows() && revision.row(end).wrapped() {
        end += 1;
    }
    (
        PointCoordinate { x: 0, y: start },
        PointCoordinate {
            x: revision.columns.saturating_sub(1),
            y: end,
        },
    )
}

fn revision_word_class(
    revision: &ModeRevision,
    point: PointCoordinate,
    word_separators: &WordSeparators,
) -> CopyWordClass {
    character_word_class(revision.first_char(point), word_separators)
}

fn character_word_class(
    character: Option<char>,
    word_separators: &WordSeparators,
) -> CopyWordClass {
    match character {
        None => CopyWordClass::Whitespace,
        Some(character) if character.is_whitespace() => CopyWordClass::Whitespace,
        Some(character) if word_separators.contains_separator(character) => {
            CopyWordClass::Separator
        }
        Some(_) => CopyWordClass::Word,
    }
}

fn move_revision_word(
    revision: &ModeRevision,
    point: PointCoordinate,
    action: &CopyModeAction,
    word_separators: &WordSeparators,
) -> PointCoordinate {
    let columns = u64::from(revision.columns);
    let total = columns
        .saturating_mul(u64::from(revision.total_rows()))
        .max(1);
    let index = u64::from(point.y)
        .saturating_mul(columns)
        .saturating_add(u64::from(point.x))
        .min(total - 1);
    let spaces_only = matches!(
        action,
        CopyModeAction::NextSpace | CopyModeAction::PreviousSpace | CopyModeAction::NextSpaceEnd
    );
    let class_at = |index: u64| {
        let point = PointCoordinate {
            x: u16::try_from(index % columns).unwrap_or(0),
            y: u32::try_from(index / columns).unwrap_or(u32::MAX),
        };
        if spaces_only {
            match revision.first_char(point) {
                None => CopyWordClass::Whitespace,
                Some(character) if character.is_whitespace() => CopyWordClass::Whitespace,
                Some(_) => CopyWordClass::Word,
            }
        } else {
            revision_word_class(revision, point, word_separators)
        }
    };
    let destination = match action {
        CopyModeAction::NextWord | CopyModeAction::NextSpace => {
            let current = class_at(index);
            let mut next = index;
            if current != CopyWordClass::Whitespace {
                next = next.saturating_add(1).min(total - 1);
                while next < total && class_at(next) == current {
                    next += 1;
                }
            }
            while next < total && class_at(next) == CopyWordClass::Whitespace {
                next += 1;
            }
            next.min(total - 1)
        }
        CopyModeAction::PreviousWord | CopyModeAction::PreviousSpace => {
            let mut previous = index.saturating_sub(1);
            while previous > 0 && class_at(previous) == CopyWordClass::Whitespace {
                previous -= 1;
            }
            let class = class_at(previous);
            while previous > 0 && class_at(previous - 1) == class {
                previous -= 1;
            }
            previous
        }
        CopyModeAction::NextWordEnd | CopyModeAction::NextSpaceEnd => {
            let mut end = index.saturating_add(1).min(total - 1);
            while end + 1 < total && class_at(end) == CopyWordClass::Whitespace {
                end += 1;
            }
            let class = class_at(end);
            while end + 1 < total && class_at(end + 1) == class {
                end += 1;
            }
            end
        }
        _ => index,
    };
    PointCoordinate {
        x: u16::try_from(destination % columns).unwrap_or(0),
        y: u32::try_from(destination / columns).unwrap_or(u32::MAX),
    }
}

fn revision_semantic_prompt_target(
    revision: &ModeRevision,
    current_row: u32,
    direction: i8,
    output: bool,
) -> Option<PointCoordinate> {
    let total = revision.total_rows();
    let prompt_row = if direction.is_negative() {
        (0..current_row)
            .rev()
            .find(|row| revision.row(*row).prompt())
    } else {
        (current_row.saturating_add(1)..total).find(|row| revision.row(*row).prompt())
    }?;
    if output {
        return revision_semantic_output_target(revision, prompt_row);
    }
    Some(PointCoordinate {
        x: (0..revision.columns)
            .find(|column| {
                revision.is_prompt(PointCoordinate {
                    x: *column,
                    y: prompt_row,
                })
            })
            .unwrap_or(0),
        y: prompt_row,
    })
}

fn revision_semantic_output_target(
    revision: &ModeRevision,
    prompt_row: u32,
) -> Option<PointCoordinate> {
    let mut saw_input = false;
    for row in prompt_row..revision.total_rows() {
        if row > prompt_row && revision.row(row).prompt() {
            break;
        }
        for column in 0..revision.columns {
            let point = PointCoordinate { x: column, y: row };
            if revision.first_char(point).is_none() {
                continue;
            }
            if revision.is_input(point) {
                saw_input = true;
            } else if saw_input && revision.is_output(point) {
                return Some(point);
            }
        }
    }
    None
}

#[cfg(test)]
fn semantic_prompt_target(
    terminal: &Terminal<'_, '_>,
    current_row: u32,
    direction: i8,
    output: bool,
) -> Result<Option<PointCoordinate>, WorkerError> {
    let total = u32::try_from(terminal.total_rows()?).unwrap_or(u32::MAX);
    let prompt_row = if direction.is_negative() {
        let mut row = current_row;
        let mut found = None;
        while row > 0 {
            row -= 1;
            if row_semantic_prompt(terminal, row)? == RowSemanticPrompt::Prompt {
                found = Some(row);
                break;
            }
        }
        found
    } else {
        let mut row = current_row.saturating_add(1);
        let mut found = None;
        while row < total {
            if row_semantic_prompt(terminal, row)? == RowSemanticPrompt::Prompt {
                found = Some(row);
                break;
            }
            row += 1;
        }
        found
    };
    let Some(prompt_row) = prompt_row else {
        return Ok(None);
    };
    if output {
        return semantic_output_target(terminal, prompt_row, total);
    }
    Ok(Some(PointCoordinate {
        x: first_semantic_cell(terminal, prompt_row, CellSemanticContent::Prompt)?.unwrap_or(0),
        y: prompt_row,
    }))
}

fn semantic_output_target(
    terminal: &Terminal<'_, '_>,
    prompt_row: u32,
    total: u32,
) -> Result<Option<PointCoordinate>, WorkerError> {
    let columns = terminal.cols()?;
    let mut saw_input = false;
    for row in prompt_row..total {
        if row > prompt_row && row_semantic_prompt(terminal, row)? == RowSemanticPrompt::Prompt {
            break;
        }
        for column in 0..columns {
            let grid_ref =
                terminal.grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?;
            let cell = grid_ref.cell()?;
            match cell.semantic_content()? {
                CellSemanticContent::Input if cell.has_text()? => saw_input = true,
                CellSemanticContent::Output if saw_input && cell.has_text()? => {
                    return Ok(Some(PointCoordinate { x: column, y: row }));
                }
                CellSemanticContent::Output
                | CellSemanticContent::Input
                | CellSemanticContent::Prompt => {}
            }
        }
    }
    Ok(None)
}

fn row_semantic_prompt(
    terminal: &Terminal<'_, '_>,
    row: u32,
) -> Result<RowSemanticPrompt, WorkerError> {
    Ok(terminal
        .grid_ref(Point::Screen(PointCoordinate { x: 0, y: row }))?
        .row()?
        .semantic_prompt()?)
}

#[cfg(test)]
fn first_semantic_cell(
    terminal: &Terminal<'_, '_>,
    row: u32,
    semantic: CellSemanticContent,
) -> Result<Option<u16>, WorkerError> {
    for column in 0..terminal.cols()? {
        let grid_ref = terminal.grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?;
        let cell = grid_ref.cell()?;
        if cell.has_text()? && cell.semantic_content()? == semantic {
            return Ok(Some(column));
        }
    }
    Ok(None)
}

fn select_semantic_output_at(
    terminal: &Terminal<'_, '_>,
    selection_state: &mut Option<SelectionState>,
    event: PointerCellEvent,
) -> Result<bool, WorkerError> {
    let clicked = terminal.grid_ref(viewport_point(terminal, event)?)?;
    let Some(clicked) = terminal.point_from_grid_ref(&clicked, PointSpace::Screen)? else {
        return Ok(false);
    };
    let mut prompt_row = clicked.y;
    loop {
        if row_semantic_prompt(terminal, prompt_row)? == RowSemanticPrompt::Prompt {
            break;
        }
        if prompt_row == 0 {
            return Ok(false);
        }
        prompt_row -= 1;
    }
    let total = u32::try_from(terminal.total_rows()?).unwrap_or(u32::MAX);
    let Some(start) = semantic_output_target(terminal, prompt_row, total)? else {
        return Ok(false);
    };
    let columns = terminal.cols()?;
    let mut end = None;
    for row in start.y..total {
        if row > start.y && row_semantic_prompt(terminal, row)? == RowSemanticPrompt::Prompt {
            break;
        }
        let first_column = if row == start.y { start.x } else { 0 };
        for column in first_column..columns {
            let grid_ref =
                terminal.grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?;
            let cell = grid_ref.cell()?;
            if cell.has_text()? && cell.semantic_content()? == CellSemanticContent::Output {
                end = Some(PointCoordinate { x: column, y: row });
            }
        }
    }
    let Some(end) = end else {
        return Ok(false);
    };
    let anchor = terminal.track_grid_ref(Point::Screen(start))?;
    let focus = terminal.track_grid_ref(Point::Screen(end))?;
    let start_ref = terminal.grid_ref(Point::Screen(start))?;
    let end_ref = terminal.grid_ref(Point::Screen(end))?;
    terminal.set_selection(Some(&Selection::new(start_ref, end_ref, false)))?;
    *selection_state = Some(SelectionState {
        anchor,
        focus,
        mode: SelectionMode::Cell,
        rectangle: false,
    });
    Ok(true)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyWordClass {
    Whitespace,
    Word,
    Separator,
}

#[cfg(test)]
fn copy_line_end(terminal: &Terminal<'_, '_>, row: u32, columns: u16) -> Result<u16, WorkerError> {
    for column in (0..columns).rev() {
        if copy_word_class(terminal, PointCoordinate { x: column, y: row })?
            != CopyWordClass::Whitespace
        {
            return Ok(column);
        }
    }
    Ok(0)
}

#[cfg(test)]
fn move_copy_word(
    terminal: &Terminal<'_, '_>,
    point: PointCoordinate,
    columns: u16,
    rows: u32,
    action: &CopyModeAction,
) -> Result<PointCoordinate, WorkerError> {
    let columns_u64 = u64::from(columns);
    let total = columns_u64.saturating_mul(u64::from(rows)).max(1);
    let index = u64::from(point.y)
        .saturating_mul(columns_u64)
        .saturating_add(u64::from(point.x))
        .min(total - 1);
    let class_at = |index: u64| {
        copy_word_class(
            terminal,
            PointCoordinate {
                x: u16::try_from(index % columns_u64).unwrap_or(0),
                y: u32::try_from(index / columns_u64).unwrap_or(u32::MAX),
            },
        )
    };

    let destination = match action {
        CopyModeAction::NextWord => {
            let current = class_at(index)?;
            let mut next = index;
            if current != CopyWordClass::Whitespace {
                next = next.saturating_add(1).min(total - 1);
                while next < total && class_at(next)? == current {
                    next += 1;
                }
            }
            while next < total && class_at(next)? == CopyWordClass::Whitespace {
                next += 1;
            }
            next.min(total - 1)
        }
        CopyModeAction::PreviousWord => {
            let mut previous = index.saturating_sub(1);
            while previous > 0 && class_at(previous)? == CopyWordClass::Whitespace {
                previous -= 1;
            }
            let class = class_at(previous)?;
            while previous > 0 && class_at(previous - 1)? == class {
                previous -= 1;
            }
            previous
        }
        CopyModeAction::NextWordEnd => {
            let mut end = index.saturating_add(1).min(total - 1);
            while end + 1 < total && class_at(end)? == CopyWordClass::Whitespace {
                end += 1;
            }
            let class = class_at(end)?;
            while end + 1 < total && class_at(end + 1)? == class {
                end += 1;
            }
            end
        }
        _ => index,
    };
    Ok(PointCoordinate {
        x: u16::try_from(destination % columns_u64).unwrap_or(0),
        y: u32::try_from(destination / columns_u64).unwrap_or(u32::MAX),
    })
}

fn write_focus_event(
    terminal: &Terminal<'_, '_>,
    focused: bool,
    writer: &mut dyn Write,
) -> Result<(), WorkerError> {
    if !terminal.mode(Mode::FOCUS_EVENT)? {
        return Ok(());
    }
    let event = if focused {
        focus::Event::Gained
    } else {
        focus::Event::Lost
    };
    let mut encoded = [0_u8; 8];
    let written = event.encode(&mut encoded)?;
    writer.write_all(&encoded[..written])?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
fn copy_word_class(
    terminal: &Terminal<'_, '_>,
    point: PointCoordinate,
) -> Result<CopyWordClass, WorkerError> {
    let grid_ref = terminal.grid_ref(Point::Screen(point))?;
    let mut stack = ['\0'; 8];
    let first = match grid_ref.graphemes(&mut stack) {
        Ok(count) => stack
            .get(..count)
            .and_then(|graphemes| graphemes.first())
            .copied(),
        Err(libghostty_vt::Error::OutOfSpace { required }) => {
            let mut graphemes = vec!['\0'; required];
            let count = grid_ref.graphemes(&mut graphemes)?;
            graphemes
                .get(..count)
                .and_then(|values| values.first())
                .copied()
        }
        Err(error) => return Err(error.into()),
    };
    Ok(match first {
        None => CopyWordClass::Whitespace,
        Some(character) if character.is_whitespace() => CopyWordClass::Whitespace,
        Some(character) if WordSeparators::default().contains_separator(character) => {
            CopyWordClass::Separator
        }
        Some(_) => CopyWordClass::Word,
    })
}

fn update_copy_selection(mode: &mut CopyModeState, word_separators: Option<&WordSeparators>) {
    let unit = mode.selection_mode;
    let origin = mode.selection_origin;
    let cursor = mode.cursor;
    let revision = Arc::clone(&mode.revision);
    let Some(selection) = mode.selection.as_mut() else {
        return;
    };
    selection.focus = cursor;
    match unit {
        CopySelectionMode::Char => {}
        CopySelectionMode::Word => {
            let Some(separators) = word_separators else {
                return;
            };
            if (cursor.y, cursor.x) >= (origin.y, origin.x) {
                selection.anchor = mode_word_edge(&revision, origin, separators, false);
                selection.focus = mode_word_edge(&revision, cursor, separators, true);
            } else {
                selection.anchor = mode_word_edge(&revision, origin, separators, true);
                selection.focus = mode_word_edge(&revision, cursor, separators, false);
            }
            selection.mode = SelectionMode::Word;
        }
        CopySelectionMode::Line => {
            let (origin_start, origin_end) = mode_logical_line_bounds(&revision, origin.y);
            let (cursor_start, cursor_end) = mode_logical_line_bounds(&revision, cursor.y);
            if cursor.y >= origin.y {
                selection.anchor = origin_start;
                selection.focus = cursor_end;
            } else {
                selection.anchor = origin_end;
                selection.focus = cursor_start;
            }
            selection.mode = SelectionMode::Line;
        }
    }
}

fn format_selection_text(terminal: &Terminal<'_, '_>) -> Result<String, WorkerError> {
    let options = FormatOptions::new()
        .with_emit_format(Format::Plain)
        .with_unwrap(true)
        .with_trim(true);
    Ok(terminal
        .format_selection_alloc(None, options)?
        .map_or_else(String::new, |bytes| {
            String::from_utf8_lossy(bytes.as_ref()).into_owned()
        }))
}

const MAX_SEARCH_QUERY_BYTES: usize = 4096;
const MAX_SEARCH_MATCHES: usize = 1_000_000;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct SearchCellOffset {
    start: u32,
    end: u32,
    column: u16,
    width: u16,
}

#[derive(Debug)]
struct HistorySearchSnapshot {
    columns: u16,
    terminal: Arc<Mutex<libghostty_vt::terminal::ScreenSnapshot>>,
    total_rows: u32,
    #[cfg(test)]
    search_gate: Mutex<()>,
}

#[derive(Clone, Copy)]
enum SearchSelectionPolicy {
    Last,
    From {
        point: PointCoordinate,
        direction: SearchDirection,
    },
    Preserve {
        index: Option<usize>,
        found: Option<SearchMatch>,
    },
}

struct SearchJob {
    request_id: u64,
    view_id: TerminalViewId,
    screen: Screen,
    query: SearchQuery,
    snapshot: Arc<HistorySearchSnapshot>,
    selection: SearchSelectionPolicy,
    match_scratch: Vec<SearchMatch>,
    latest_request: Arc<AtomicU64>,
}

struct SearchResult {
    request_id: u64,
    view_id: TerminalViewId,
    screen: Screen,
    state: SearchState,
}

#[derive(Default)]
struct SearchJobs {
    by_view: HashMap<TerminalViewId, SearchJob>,
}

#[derive(Default)]
struct SearchResults {
    by_view: HashMap<TerminalViewId, SearchResult>,
}

struct SearchWorker {
    jobs: Sender<SearchJobs>,
    discard_jobs: Receiver<SearchJobs>,
    latest_requests: HashMap<TerminalViewId, Arc<AtomicU64>>,
    next_request: u64,
    match_scratch: Vec<SearchMatch>,
    mailbox: Arc<SearchMailbox>,
}

struct SearchMailbox {
    queued: AtomicBool,
    jobs: Receiver<SearchJobs>,
    results: Sender<SearchResults>,
    discard_results: Receiver<SearchResults>,
    wake: ActorWake,
}

static SEARCH_SCHEDULER: LazyLock<Sender<Arc<SearchMailbox>>> = LazyLock::new(|| {
    let (ready, mailboxes) = crossbeam_channel::unbounded();
    thread::Builder::new()
        .name("zz-terminal-search".into())
        .spawn(move || search_worker(&mailboxes))
        .expect("could not start the terminal search thread");
    ready
});

impl SearchWorker {
    fn spawn(wake: ActorWake) -> (Self, Receiver<SearchResults>) {
        let (jobs, job_rx) = crossbeam_channel::bounded::<SearchJobs>(1);
        let discard_jobs = job_rx.clone();
        let (result_tx, results) = crossbeam_channel::bounded::<SearchResults>(1);
        let discard_results = results.clone();
        (
            Self {
                jobs,
                discard_jobs,
                latest_requests: HashMap::new(),
                next_request: 0,
                match_scratch: Vec::new(),
                mailbox: Arc::new(SearchMailbox {
                    queued: AtomicBool::new(false),
                    jobs: job_rx,
                    results: result_tx,
                    discard_results,
                    wake,
                }),
            },
            results,
        )
    }

    fn next_request(&mut self, view_id: TerminalViewId) -> (u64, Arc<AtomicU64>) {
        self.next_request = self.next_request.wrapping_add(1).max(1);
        let latest = self
            .latest_requests
            .entry(view_id)
            .or_insert_with(|| Arc::new(AtomicU64::new(0)));
        latest.store(self.next_request, Ordering::Release);
        (self.next_request, Arc::clone(latest))
    }

    fn cancel(&mut self, view_id: TerminalViewId) -> u64 {
        let (request_id, _) = self.next_request(view_id);
        if let Ok(mut pending) = self.discard_jobs.try_recv() {
            if let Some(mut discarded) = pending.by_view.remove(&view_id) {
                keep_larger_match_scratch(&mut self.match_scratch, &mut discarded.match_scratch);
            }
            if !pending.by_view.is_empty() && self.jobs.try_send(pending).is_ok() {
                self.schedule();
            }
        }
        request_id
    }

    fn forget(&mut self, view_id: TerminalViewId) {
        self.cancel(view_id);
        self.latest_requests.remove(&view_id);
    }

    fn is_current(&self, view_id: TerminalViewId, request_id: u64) -> bool {
        self.latest_requests
            .get(&view_id)
            .is_some_and(|latest| latest.load(Ordering::Acquire) == request_id)
    }

    fn recycle_matches(&mut self, matches: &mut Vec<SearchMatch>) {
        keep_larger_match_scratch(&mut self.match_scratch, matches);
    }

    fn schedule(&self) {
        if !self.mailbox.queued.swap(true, Ordering::AcqRel) {
            let _ = SEARCH_SCHEDULER.send(Arc::clone(&self.mailbox));
        }
    }

    fn submit(&mut self, job: SearchJob) {
        let mut pending = SearchJobs::default();
        pending.by_view.insert(job.view_id, job);
        loop {
            match self.jobs.try_send(pending) {
                Ok(()) => {
                    self.schedule();
                    return;
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => return,
                Err(crossbeam_channel::TrySendError::Full(returned)) => {
                    pending = returned;
                    if let Ok(older) = self.discard_jobs.try_recv() {
                        merge_older_search_jobs(&mut pending, older);
                    }
                }
            }
        }
    }
}

fn merge_older_search_jobs(newer: &mut SearchJobs, older: SearchJobs) {
    for (view_id, mut job) in older.by_view {
        if let Some(replacement) = newer.by_view.get_mut(&view_id) {
            keep_larger_match_scratch(&mut replacement.match_scratch, &mut job.match_scratch);
        } else {
            newer.by_view.insert(view_id, job);
        }
    }
}

impl Drop for SearchWorker {
    fn drop(&mut self) {
        for latest in self.latest_requests.values() {
            latest.store(0, Ordering::Release);
        }
    }
}

fn search_worker(mailboxes: &Receiver<Arc<SearchMailbox>>) {
    let mut match_scratch = Vec::new();
    while let Ok(mailbox) = mailboxes.recv() {
        mailbox.queued.store(false, Ordering::Release);
        let Ok(pending) = mailbox.jobs.try_recv() else {
            continue;
        };
        let mut pending = pending.by_view.into_values().collect::<Vec<_>>();
        pending.sort_by_key(|job| job.view_id.0);
        for mut job in pending {
            keep_larger_match_scratch(&mut match_scratch, &mut job.match_scratch);
            if job.latest_request.load(Ordering::Acquire) != job.request_id {
                continue;
            }
            let Some(mut state) =
                job.snapshot
                    .search_reusing(&job.query, job.request_id, &mut match_scratch, || {
                        job.latest_request.load(Ordering::Acquire) != job.request_id
                    })
            else {
                continue;
            };
            select_search_result(&mut state, job.selection);
            if job.latest_request.load(Ordering::Acquire) != job.request_id {
                keep_larger_match_scratch(&mut match_scratch, &mut state.matches);
                continue;
            }
            let mut completed = SearchResults::default();
            completed.by_view.insert(
                job.view_id,
                SearchResult {
                    request_id: job.request_id,
                    view_id: job.view_id,
                    screen: job.screen,
                    state,
                },
            );
            if send_latest_search_results(
                &mailbox.results,
                &mailbox.discard_results,
                completed,
                &mut match_scratch,
            ) {
                mailbox.wake.notify();
            }
        }
    }
}

#[cfg(test)]
mod search_worker_tests;

fn send_latest_search_results(
    results: &Sender<SearchResults>,
    discard_results: &Receiver<SearchResults>,
    mut completed: SearchResults,
    match_scratch: &mut Vec<SearchMatch>,
) -> bool {
    loop {
        match results.try_send(completed) {
            Ok(()) => return true,
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => return false,
            Err(crossbeam_channel::TrySendError::Full(returned)) => {
                completed = returned;
                if let Ok(older) = discard_results.try_recv() {
                    for (view_id, mut result) in older.by_view {
                        match completed.by_view.entry(view_id) {
                            Entry::Occupied(_) => {
                                keep_larger_match_scratch(match_scratch, &mut result.state.matches);
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(result);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn keep_larger_match_scratch(target: &mut Vec<SearchMatch>, candidate: &mut Vec<SearchMatch>) {
    target.clear();
    candidate.clear();
    if candidate.capacity() > target.capacity() {
        std::mem::swap(target, candidate);
    }
}

fn select_search_result(state: &mut SearchState, selection: SearchSelectionPolicy) {
    if state.matches.is_empty() {
        return;
    }
    state.current = match selection {
        SearchSelectionPolicy::Last => Some(state.matches.len() - 1),
        SearchSelectionPolicy::From { point, direction } => match direction {
            SearchDirection::Forward => state
                .matches
                .iter()
                .position(|found| (found.row, found.start) > (point.y, point.x))
                .or(Some(0)),
            SearchDirection::Backward => state
                .matches
                .iter()
                .rposition(|found| (found.row, found.start) < (point.y, point.x))
                .or(Some(state.matches.len() - 1)),
        },
        SearchSelectionPolicy::Preserve { index, found } => found
            .and_then(|found| {
                state
                    .matches
                    .iter()
                    .position(|candidate| *candidate == found)
            })
            .or_else(|| index.map(|index| index.min(state.matches.len() - 1)))
            .or_else(|| Some(state.matches.len() - 1)),
    };
}

struct SearchTarget<'a> {
    view_id: TerminalViewId,
    screen: Screen,
    search: &'a mut SearchSlot,
    snapshot: &'a mut Option<Arc<HistorySearchSnapshot>>,
}

fn search_selection_policy(
    copy_mode: Option<&CopyModeState>,
    origin: &mut Option<PointCoordinate>,
    direction: SearchDirection,
    reset_origin: bool,
) -> SearchSelectionPolicy {
    if reset_origin {
        *origin = None;
    }
    if origin.is_none()
        && let Some(mode) = copy_mode
    {
        *origin = Some(mode.cursor);
    }
    let Some(origin) = origin.as_ref() else {
        return SearchSelectionPolicy::Last;
    };
    SearchSelectionPolicy::From {
        point: *origin,
        direction,
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the target bundles exclusive view-state borrows for one queued request"
)]
fn queue_search(
    terminal: &Terminal<'_, '_>,
    target: SearchTarget<'_>,
    worker: &mut SearchWorker,
    query: SearchQuery,
    selection: SearchSelectionPolicy,
) -> Result<(), WorkerError> {
    let (request_id, latest_request) = worker.next_request(target.view_id);
    let searchable = !query.text.is_empty() && query.text.len() <= MAX_SEARCH_QUERY_BYTES;
    let invalid_pattern = query.text.len() > MAX_SEARCH_QUERY_BYTES;
    if searchable && target.snapshot.is_none() {
        *target.snapshot = Some(Arc::new(HistorySearchSnapshot::capture(terminal)?));
    }
    let mut match_scratch = target
        .search
        .as_mut()
        .map_or_else(Vec::new, |state| std::mem::take(&mut state.matches));
    store_search_state(
        target.search,
        SearchState {
            query: query.clone(),
            matches: if searchable {
                Vec::new()
            } else {
                std::mem::take(&mut match_scratch)
            },
            request_id,
            pending: searchable,
            invalid_pattern,
            ..SearchState::default()
        },
    );
    if !searchable {
        return Ok(());
    }
    worker.submit(SearchJob {
        request_id,
        view_id: target.view_id,
        screen: target.screen,
        query,
        snapshot: Arc::clone(target.snapshot.as_ref().expect("captured above")),
        selection,
        match_scratch,
        latest_request,
    });
    Ok(())
}

fn refresh_view_search(
    terminal: &Terminal<'_, '_>,
    view_id: TerminalViewId,
    view: &mut TerminalViewState,
    worker: &mut SearchWorker,
) -> Result<bool, WorkerError> {
    let Some(previous) = view.search.as_ref() else {
        worker.cancel(view_id);
        return Ok(false);
    };
    let query = previous.query.clone();
    let selection = SearchSelectionPolicy::Preserve {
        index: previous.current,
        found: previous
            .current
            .and_then(|index| previous.matches.get(index))
            .copied(),
    };
    let screen = view.screen;
    let current = view.active_mut();
    if current.search_snapshot.is_none()
        && let Some(mode) = current.copy_mode.as_ref()
    {
        current.search_snapshot = Some(Arc::clone(&mode.revision.search));
    }
    queue_search(
        terminal,
        SearchTarget {
            view_id,
            screen,
            search: &mut current.search,
            snapshot: &mut current.search_snapshot,
        },
        worker,
        query,
        selection,
    )?;
    Ok(true)
}

fn apply_search_result(
    terminal: &mut Terminal<'_, '_>,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
    worker: &mut SearchWorker,
    mut result: SearchResult,
) -> Result<bool, WorkerError> {
    if !worker.is_current(result.view_id, result.request_id) {
        worker.recycle_matches(&mut result.state.matches);
        return Ok(false);
    }
    let is_active = active.contains_key(&result.view_id);
    let view = if let Some(view) = active.get_mut(&result.view_id) {
        view
    } else if let Some(view) = inactive.get_mut(&result.view_id) {
        view
    } else {
        worker.recycle_matches(&mut result.state.matches);
        return Ok(false);
    };
    let current_screen = view.screen;
    let screen = view.screen_mut(result.screen);
    let Some(pending) = screen.search.as_mut() else {
        worker.recycle_matches(&mut result.state.matches);
        return Ok(false);
    };
    if pending.request_id != result.request_id || pending.query != result.state.query {
        worker.recycle_matches(&mut result.state.matches);
        return Ok(false);
    }
    worker.recycle_matches(&mut pending.matches);
    store_search_state(&mut screen.search, result.state);
    if !is_active || current_screen != result.screen {
        return Ok(false);
    }
    {
        let current = view.active_mut();
        sync_copy_cursor_to_search(&mut current.copy_mode, current.search.as_deref());
        reveal_search_for_view(
            terminal,
            current.copy_mode.as_deref(),
            current.search.as_deref(),
        )?;
    }
    sync_viewport_anchor(terminal, view)?;
    Ok(true)
}

fn apply_search_results(
    terminal: &mut Terminal<'_, '_>,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
    worker: &mut SearchWorker,
    results: SearchResults,
) -> Result<bool, WorkerError> {
    let mut results = results.by_view.into_values().collect::<Vec<_>>();
    results.sort_by_key(|result| result.view_id.0);
    let mut changed = false;
    for result in results {
        changed |= apply_search_result(terminal, active, inactive, worker, result)?;
    }
    Ok(changed)
}

fn complete_view_search(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<(), WorkerError> {
    let Some(previous) = view.search.as_ref() else {
        return Ok(());
    };
    let query = previous.query.clone();
    let request_id = previous.request_id;
    let selection = SearchSelectionPolicy::Preserve {
        index: previous.current,
        found: previous
            .current
            .and_then(|index| previous.matches.get(index))
            .copied(),
    };
    let snapshot = view.copy_mode.as_ref().map_or_else(
        || HistorySearchSnapshot::capture(terminal).map(Arc::new),
        |mode| Ok(Arc::clone(&mode.revision.search)),
    )?;
    let mut state = snapshot
        .search(&query, request_id, || false)
        .expect("inline terminal search is never cancelled");
    select_search_result(&mut state, selection);
    view.search_snapshot = Some(snapshot);
    store_search_state(&mut view.search, state);
    {
        let current = view.active_mut();
        sync_copy_cursor_to_search(&mut current.copy_mode, current.search.as_deref());
        reveal_search_for_view(
            terminal,
            current.copy_mode.as_deref(),
            current.search.as_deref(),
        )?;
    }
    sync_viewport_anchor(terminal, view)
}

struct SearchSnapshotCompression<'a>(&'a HistorySearchSnapshot);

impl Drop for SearchSnapshotCompression<'_> {
    fn drop(&mut self) {
        let _ = self.0.terminal.lock().compress(CompressionMode::Full);
    }
}

impl HistorySearchSnapshot {
    fn capture(terminal: &Terminal<'_, '_>) -> Result<Self, WorkerError> {
        Ok(Self {
            columns: terminal.cols()?,
            total_rows: u32::try_from(terminal.total_rows()?)
                .map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
            terminal: Arc::new(Mutex::new(terminal.clone_screen()?)),
            #[cfg(test)]
            search_gate: Mutex::new(()),
        })
    }

    fn search(
        &self,
        query: &SearchQuery,
        request_id: u64,
        cancelled: impl Fn() -> bool,
    ) -> Option<SearchState> {
        let mut match_scratch = Vec::new();
        self.search_reusing(query, request_id, &mut match_scratch, cancelled)
    }

    fn search_reusing(
        &self,
        query: &SearchQuery,
        request_id: u64,
        match_scratch: &mut Vec<SearchMatch>,
        cancelled: impl Fn() -> bool,
    ) -> Option<SearchState> {
        #[cfg(test)]
        let _search_gate = self.search_gate.lock();
        match_scratch.clear();
        if cancelled() {
            return None;
        }
        if query.text.is_empty() || query.text.len() > MAX_SEARCH_QUERY_BYTES {
            return Some(SearchState {
                query: query.clone(),
                matches: std::mem::take(match_scratch),
                request_id,
                invalid_pattern: query.text.len() > MAX_SEARCH_QUERY_BYTES,
                ..SearchState::default()
            });
        }
        let pattern = match query.mode {
            SearchMode::Literal => regex::escape(&query.text),
            SearchMode::Regex => query.text.clone(),
        };
        let case_sensitive = match query.case {
            SearchCase::Smart => query.text.chars().any(char::is_uppercase),
            SearchCase::Sensitive => true,
            SearchCase::Insensitive => false,
        };
        let Ok(expression) = RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .size_limit(1024 * 1024)
            .dfa_size_limit(1024 * 1024)
            .build()
        else {
            return Some(SearchState {
                query: query.clone(),
                matches: std::mem::take(match_scratch),
                request_id,
                invalid_pattern: true,
                ..SearchState::default()
            });
        };
        let _compression = SearchSnapshotCompression(self);
        let mut text = String::new();
        let mut offsets = Vec::new();
        let mut graphemes = Vec::new();
        let mut wrapped_rows = Vec::new();
        for row in 0..self.total_rows {
            if cancelled() {
                return None;
            }
            {
                let mut terminal = self.terminal.lock();
                if row > 0 && row % 512 == 0 {
                    terminal.compress(CompressionMode::Full).ok()?;
                }
                let text_start = u32::try_from(text.len()).ok()?;
                let offset_start = offsets.len();
                let wrapped = append_history_row(
                    &*terminal,
                    row,
                    self.columns,
                    &mut text,
                    &mut offsets,
                    &mut graphemes,
                )
                .ok()?;
                for offset in &mut offsets[offset_start..] {
                    offset.start = offset.start.checked_add(text_start)?;
                    offset.end = offset.end.checked_add(text_start)?;
                }
                wrapped_rows.push((row, offsets.len()));
                if wrapped && row.saturating_add(1) < self.total_rows {
                    continue;
                }
            }
            for found in expression.find_iter(&text) {
                let Some((start, end)) =
                    search_match_span(&offsets, found.start(), found.end(), self.columns)
                else {
                    continue;
                };
                let (start_row, end_row) = {
                    let first = offsets.partition_point(|cell| {
                        usize::try_from(cell.end).is_ok_and(|end| end <= found.start())
                    });
                    let after_last = offsets.partition_point(|cell| {
                        usize::try_from(cell.start).is_ok_and(|start| start < found.end())
                    });
                    let start = wrapped_rows.partition_point(|(_, end)| *end <= first);
                    let end = wrapped_rows.partition_point(|(_, end)| *end < after_last);
                    (wrapped_rows.get(start)?.0, wrapped_rows.get(end)?.0)
                };
                match_scratch.push(SearchMatch {
                    row: start_row,
                    end_row,
                    start,
                    end,
                });
                if match_scratch.len() >= MAX_SEARCH_MATCHES {
                    return Some(SearchState {
                        query: query.clone(),
                        matches: std::mem::take(match_scratch),
                        request_id,
                        ..SearchState::default()
                    });
                }
            }
            text.clear();
            offsets.clear();
            wrapped_rows.clear();
        }
        Some(SearchState {
            query: query.clone(),
            matches: std::mem::take(match_scratch),
            request_id,
            ..SearchState::default()
        })
    }
}

fn search_match_span(
    offsets: &[SearchCellOffset],
    byte_start: usize,
    byte_end: usize,
    columns: u16,
) -> Option<(u16, u16)> {
    if byte_start >= byte_end {
        return None;
    }
    let first = offsets
        .partition_point(|cell| usize::try_from(cell.end).is_ok_and(|end| end <= byte_start));
    let after_last = offsets
        .partition_point(|cell| usize::try_from(cell.start).is_ok_and(|start| start < byte_end));
    if first >= after_last {
        return None;
    }
    let first = offsets.get(first)?;
    let last = offsets.get(after_last - 1)?;
    Some((
        first.column,
        last.column.saturating_add(last.width).min(columns),
    ))
}

#[cfg(test)]
fn search_history(terminal: &Terminal<'_, '_>, query: &str) -> Result<SearchState, WorkerError> {
    Ok(HistorySearchSnapshot::capture(terminal)?
        .search(&SearchQuery::literal(query), 1, || false)
        .expect("synchronous fixture is never cancelled"))
}

fn append_history_row(
    terminal: &impl libghostty_vt::terminal::GridRead,
    row: u32,
    columns: u16,
    text: &mut String,
    offsets: &mut Vec<SearchCellOffset>,
    grapheme_scratch: &mut Vec<char>,
) -> Result<bool, WorkerError> {
    let row_text_start = text.len();
    let mut stack = ['\0'; 8];
    let native_row = terminal.grid_row(Point::Screen(PointCoordinate { x: 0, y: row }))?;
    let wrapped = native_row.row()?.is_wrapped()?;
    for column in 0..columns {
        let grid_ref = native_row.cell(column).expect("history column");
        let wide = grid_ref.cell()?.wide()?;
        if matches!(wide, CellWide::SpacerTail | CellWide::SpacerHead) {
            continue;
        }
        let start = u32::try_from(text.len().saturating_sub(row_text_start))
            .map_err(|_| WorkerError::SearchSnapshotTooLarge)?;
        match grid_ref.graphemes(&mut stack) {
            Ok(0) => {}
            Ok(1) => text.push(stack[0]),
            Ok(count) => text.extend(stack[..count].iter()),
            Err(libghostty_vt::Error::OutOfSpace { required }) => {
                if required > MAX_SEARCH_SNAPSHOT_BYTES / std::mem::size_of::<char>() {
                    return Err(WorkerError::SearchSnapshotTooLarge);
                }
                if grapheme_scratch.len() < required {
                    grapheme_scratch.resize(required, '\0');
                }
                let count = grid_ref.graphemes(grapheme_scratch)?;
                text.extend(grapheme_scratch[..count].iter());
            }
            Err(error) => return Err(error.into()),
        }
        let end = u32::try_from(text.len().saturating_sub(row_text_start))
            .map_err(|_| WorkerError::SearchSnapshotTooLarge)?;
        if end > start {
            offsets.push(SearchCellOffset {
                start,
                end,
                column,
                width: if matches!(wide, CellWide::Wide) { 2 } else { 1 },
            });
        }
    }
    Ok(wrapped)
}

fn remember_copy_mode_search(
    mode: Option<&mut CopyModeState>,
    pane_search: &mut Option<CopyModeSearch>,
    query: &SearchQuery,
) {
    if query.text.is_empty() {
        return;
    }
    if let Some(mode) = mode {
        let search = CopyModeSearch {
            text: query.text.clone(),
            direction: query.direction,
            regex: query.mode == SearchMode::Regex,
            incremental: false,
        };
        mode.search = Some(search.clone());
        *pane_search = Some(search);
    }
}

fn copy_mode_search_again(mode: Option<&CopyModeState>, reverse: bool) -> Option<CopyModeSearch> {
    let stored = mode?.search.as_ref()?;
    let direction = match (stored.direction, reverse) {
        (SearchDirection::Forward, false) | (SearchDirection::Backward, true) => {
            SearchDirection::Forward
        }
        (SearchDirection::Backward, false) | (SearchDirection::Forward, true) => {
            SearchDirection::Backward
        }
    };
    Some(CopyModeSearch {
        direction,
        incremental: false,
        ..stored.clone()
    })
}

fn pick_copy_search_match(
    matches: &[SearchMatch],
    cursor: PointCoordinate,
    direction: SearchDirection,
    inclusive: bool,
    wrap: bool,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    let found = match direction {
        SearchDirection::Forward => matches.iter().position(|found| {
            if inclusive {
                (found.row, found.start) >= (cursor.y, cursor.x)
            } else {
                (found.row, found.start) > (cursor.y, cursor.x)
            }
        }),
        SearchDirection::Backward => matches
            .iter()
            .rposition(|found| (found.row, found.start) < (cursor.y, cursor.x)),
    };
    if found.is_some() {
        return found;
    }
    if !wrap {
        return None;
    }
    match direction {
        SearchDirection::Forward => Some(0),
        SearchDirection::Backward => Some(matches.len() - 1),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "one call site threads the view's search state"
)]
fn run_copy_mode_search(
    view_id: TerminalViewId,
    copy_mode: &mut CopyModeSlot,
    search: &mut SearchSlot,
    search_snapshot: &mut Option<Arc<HistorySearchSnapshot>>,
    worker: &mut SearchWorker,
    spec: &CopyModeSearch,
    count: u32,
    mode_keys_vi: bool,
    wrap: bool,
    pane_search: &mut Option<CopyModeSearch>,
) -> bool {
    let Some(mode) = copy_mode.as_deref_mut() else {
        return false;
    };
    if spec.incremental {
        match mode.incremental_origin {
            None => {
                mode.incremental_origin = Some(CopyModeIncrementalOrigin {
                    row: mode.cursor.y,
                    viewport_offset: mode.viewport_offset,
                });
            }
            Some(origin) => {
                if mode
                    .search
                    .as_ref()
                    .is_some_and(|previous| previous.text != spec.text)
                {
                    mode.viewport_offset = origin.viewport_offset;
                    mode.cursor = PointCoordinate {
                        x: copy_cursor_limit(&mode.revision, origin.row, mode_keys_vi, false),
                        y: origin.row,
                    };
                }
            }
        }
        if spec.text.is_empty() {
            mode.search_marks = false;
            mode.search_count = None;
            *search = None;
            return true;
        }
    } else if spec.text.is_empty() {
        return false;
    }

    let regex = spec.regex && spec.text.contains(|c| "^$*+()?[].\\".contains(c));
    let visible_only = !mode.search_all
        && pane_search
            .as_ref()
            .is_some_and(|stored| stored.text == spec.text && stored.regex == regex);
    mode.search_all = false;
    if !visible_only && mode.search_marks {
        mode.search_marks = false;
        mode.search_count = None;
    }
    mode.search = Some(spec.clone());
    *pane_search = Some(CopyModeSearch {
        regex,
        incremental: false,
        ..spec.clone()
    });
    let query = SearchQuery {
        text: spec.text.clone(),
        mode: if regex {
            SearchMode::Regex
        } else {
            SearchMode::Literal
        },
        case: SearchCase::Smart,
        direction: spec.direction,
    };
    if search_snapshot.is_none() {
        *search_snapshot = Some(Arc::clone(&mode.revision.search));
    }
    let snapshot = Arc::clone(search_snapshot.as_ref().expect("captured above"));
    let (request_id, _) = worker.next_request(view_id);
    let mut scratch = Vec::new();
    let Some(mut state) = snapshot.search_reusing(&query, request_id, &mut scratch, || false)
    else {
        return false;
    };
    let inclusive = !mode_keys_vi && spec.direction == SearchDirection::Forward;
    let mut placed = None;
    let mut cursor = mode.cursor;
    for _ in 0..count.max(1) {
        let Some(index) =
            pick_copy_search_match(&state.matches, cursor, spec.direction, inclusive, wrap)
        else {
            break;
        };
        let Some(found) = state.matches.get(index).copied() else {
            break;
        };
        cursor = PointCoordinate {
            x: if inclusive { found.end } else { found.start },
            y: if inclusive { found.end_row } else { found.row },
        };
        placed = Some(index);
    }
    let Some(index) = placed else {
        state.current = None;
        store_search_state(search, state);
        return true;
    };
    state.current = Some(index);
    let total = u32::try_from(state.matches.len()).unwrap_or(u32::MAX);
    store_search_state(search, state);
    mode.search_marks = true;
    if !visible_only {
        mode.search_count = Some((total, false));
    }
    place_copy_cursor(mode, cursor, mode_keys_vi);
    scroll_copy_view_to_cursor(mode);
    if mode.selecting {
        update_copy_selection(mode, None);
    }
    true
}

fn step_search(search: &mut SearchSlot, direction: isize, wrap: bool) {
    let Some(search) = search.as_mut() else {
        return;
    };
    if search.matches.is_empty() {
        search.current = None;
        return;
    }
    let len = isize::try_from(search.matches.len()).unwrap_or(isize::MAX);
    let current = isize::try_from(search.current.unwrap_or(0)).unwrap_or(0);
    let next = if wrap {
        current.saturating_add(direction).rem_euclid(len)
    } else {
        current.saturating_add(direction).clamp(0, len - 1)
    };
    search.current = usize::try_from(next).ok();
}

fn sync_copy_cursor_to_search(copy_mode: &mut CopyModeSlot, search: Option<&SearchState>) {
    let Some(mode) = copy_mode.as_mut() else {
        return;
    };
    let Some(found) = search.and_then(|search| {
        search
            .current
            .and_then(|current| search.matches.get(current))
    }) else {
        return;
    };
    mode.cursor = mode.revision.clamp_point(PointCoordinate {
        x: found.start,
        y: found.row,
    });
    reveal_copy_cursor(mode);
    if mode.selecting {
        update_copy_selection(mode, None);
    }
}

fn reveal_search_for_view(
    terminal: &mut Terminal<'_, '_>,
    copy_mode: Option<&CopyModeState>,
    search: Option<&SearchState>,
) -> Result<(), WorkerError> {
    if copy_mode.is_none()
        && let Some(search) = search
    {
        reveal_search_match(terminal, search)?;
    }
    Ok(())
}

fn reveal_search_match(
    terminal: &mut Terminal<'_, '_>,
    search: &SearchState,
) -> Result<(), WorkerError> {
    let Some(found) = search
        .current
        .and_then(|current| search.matches.get(current))
    else {
        return Ok(());
    };
    let half_page = u32::from(terminal.rows()?) / 2;
    let top = found.row.saturating_sub(half_page);
    terminal.scroll_viewport(ScrollViewport::Top);
    terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(i64::from(top))));
    Ok(())
}

fn grid_ref_first_character(
    grid_ref: &libghostty_vt::screen::GridRef<'_>,
) -> Result<Option<char>, WorkerError> {
    if !grid_ref.cell()?.has_text()? {
        return Ok(None);
    }
    let mut stack = ['\0'; 8];
    match grid_ref.graphemes(&mut stack) {
        Ok(count) => Ok(stack
            .get(..count)
            .and_then(|values| values.first())
            .copied()),
        Err(libghostty_vt::Error::OutOfSpace { required }) => {
            let mut graphemes = vec!['\0'; required];
            let count = grid_ref.graphemes(&mut graphemes)?;
            Ok(graphemes
                .get(..count)
                .and_then(|values| values.first())
                .copied())
        }
        Err(error) => Err(error.into()),
    }
}

fn select_native_word<'terminal>(
    terminal: &'terminal Terminal<'_, '_>,
    grid_ref: libghostty_vt::screen::GridRef<'terminal>,
    word_separators: &WordSeparators,
) -> Result<Option<Selection<'terminal>>, WorkerError> {
    let Some(character) = grid_ref_first_character(&grid_ref)? else {
        return Ok(None);
    };
    let boundaries = match character_word_class(Some(character), word_separators) {
        CopyWordClass::Whitespace => WordSeparators::whitespace_codepoints(),
        CopyWordClass::Separator => word_separators.separator_codepoints(),
        CopyWordClass::Word => word_separators.boundary_codepoints(),
    };
    Ok(terminal
        .select_word(SelectWordOptions::new(grid_ref).with_boundary_codepoints(boundaries))?)
}

fn select_native_word_between<'terminal>(
    terminal: &'terminal Terminal<'_, '_>,
    start: &libghostty_vt::screen::GridRef<'terminal>,
    end: &libghostty_vt::screen::GridRef<'terminal>,
    word_separators: &WordSeparators,
) -> Result<Option<Selection<'terminal>>, WorkerError> {
    let Some(start) = terminal.point_from_grid_ref(start, PointSpace::Screen)? else {
        return Ok(None);
    };
    let Some(end) = terminal.point_from_grid_ref(end, PointSpace::Screen)? else {
        return Ok(None);
    };
    let columns = u64::from(terminal.cols()?);
    let mut index = u64::from(start.y)
        .saturating_mul(columns)
        .saturating_add(u64::from(start.x));
    let end = u64::from(end.y)
        .saturating_mul(columns)
        .saturating_add(u64::from(end.x));
    loop {
        let grid_ref = terminal.grid_ref(Point::Screen(PointCoordinate {
            x: u16::try_from(index % columns).unwrap_or(0),
            y: u32::try_from(index / columns).unwrap_or(u32::MAX),
        }))?;
        if grid_ref.cell()?.has_text()? {
            return select_native_word(terminal, grid_ref, word_separators);
        }
        if index == end {
            return Ok(None);
        }
        if index < end {
            index = index.saturating_add(1);
        } else {
            index = index.saturating_sub(1);
        }
    }
}

fn selection_press(
    terminal: &Terminal<'_, '_>,
    state: &mut Option<SelectionState>,
    event: PointerCellEvent,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let point = viewport_point(terminal, event)?;
    let anchor = terminal.track_grid_ref(point)?;
    let focus = terminal.track_grid_ref(point)?;
    let grid_ref = terminal.grid_ref(point)?;
    let mode = match event.click_count {
        0 | 1 => SelectionMode::Cell,
        2 => SelectionMode::Word,
        _ => SelectionMode::Line,
    };
    let selection = match mode {
        SelectionMode::Cell => None,
        SelectionMode::Word => select_native_word(terminal, grid_ref, word_separators)?,
        SelectionMode::Line => terminal.select_line(SelectLineOptions::new(grid_ref))?,
    };
    terminal.set_selection(selection.as_ref())?;
    *state = Some(SelectionState {
        anchor,
        focus,
        mode,
        rectangle: event.rectangle,
    });
    Ok(())
}

fn select_all_history(
    terminal: &Terminal<'_, '_>,
    state: &mut Option<SelectionState>,
) -> Result<(), WorkerError> {
    let columns = terminal.cols()?;
    let rows = u32::try_from(terminal.total_rows()?).unwrap_or(u32::MAX);
    let mut end = None;
    'rows: for row in (0..rows).rev() {
        for column in (0..columns).rev() {
            let point = PointCoordinate { x: column, y: row };
            if terminal
                .grid_ref(Point::Screen(point))?
                .cell()?
                .has_text()?
            {
                end = Some(point);
                break 'rows;
            }
        }
    }
    let Some(end) = end else {
        *state = None;
        terminal.set_selection(None)?;
        return Ok(());
    };
    let start = PointCoordinate { x: 0, y: 0 };
    let anchor = terminal.track_grid_ref(Point::Screen(start))?;
    let focus = terminal.track_grid_ref(Point::Screen(end))?;
    let start_ref = terminal.grid_ref(Point::Screen(start))?;
    let end_ref = terminal.grid_ref(Point::Screen(end))?;
    terminal.set_selection(Some(&Selection::new(start_ref, end_ref, false)))?;
    *state = Some(SelectionState {
        anchor,
        focus,
        mode: SelectionMode::Cell,
        rectangle: false,
    });
    Ok(())
}

fn selection_drag(
    terminal: &Terminal<'_, '_>,
    state: &mut Option<SelectionState>,
    event: PointerCellEvent,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let current = terminal.grid_ref(viewport_point(terminal, event)?)?;
    selection_drag_to(terminal, state, current, word_separators)
}

fn selection_drag_to(
    terminal: &Terminal<'_, '_>,
    state: &mut Option<SelectionState>,
    current: libghostty_vt::screen::GridRef<'_>,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let Some(selection_state) = state.as_ref() else {
        return Ok(());
    };
    let Some(current_point) = terminal.point_from_grid_ref(&current, PointSpace::Screen)? else {
        return Ok(());
    };
    let Some(anchor) = selection_state.anchor.snapshot(terminal)? else {
        *state = None;
        terminal.set_selection(None)?;
        return Ok(());
    };
    let anchor_point = terminal.point_from_grid_ref(&anchor, PointSpace::Screen)?;
    let next = match selection_state.mode {
        SelectionMode::Cell => anchor_point
            .is_some_and(|anchor| anchor != current_point)
            .then(|| Selection::new(anchor, current, selection_state.rectangle)),
        SelectionMode::Word => {
            let start = select_native_word_between(terminal, &anchor, &current, word_separators)?;
            let end = select_native_word_between(terminal, &current, &anchor, word_separators)?;
            start
                .zip(end)
                .map(|(start, end)| Selection::new(start.start(), end.end(), false))
        }
        SelectionMode::Line => {
            let start = terminal.select_line(SelectLineOptions::new(anchor))?;
            let end = terminal.select_line(SelectLineOptions::new(current))?;
            start
                .zip(end)
                .map(|(start, end)| Selection::new(start.start(), end.end(), false))
        }
    };
    terminal.set_selection(next.as_ref())?;
    let focus = terminal.track_grid_ref(Point::Screen(current_point))?;
    if let Some(selection_state) = state.as_mut() {
        selection_state.focus = focus;
    }
    Ok(())
}

fn viewport_point(
    terminal: &Terminal<'_, '_>,
    event: PointerCellEvent,
) -> Result<Point, WorkerError> {
    let column = event.column.min(terminal.cols()?.saturating_sub(1));
    let row = event.row.min(terminal.rows()?.saturating_sub(1));
    Ok(Point::Viewport(PointCoordinate {
        x: column,
        y: u32::from(row),
    }))
}

fn saturating_isize(value: i64) -> isize {
    isize::try_from(value).unwrap_or(if value.is_negative() {
        isize::MIN
    } else {
        isize::MAX
    })
}

fn saturating_isize_i128(value: i128) -> isize {
    isize::try_from(value).unwrap_or(if value.is_negative() {
        isize::MIN
    } else {
        isize::MAX
    })
}

enum Wake {
    Command(Command),
    Input(QueuedInput),
    CommandsClosed,
    Search(SearchResults),
    ChildExit(std::io::Result<ExitStatus>),
    #[cfg(unix)]
    PtyReadable,
    #[cfg(any(target_os = "linux", not(unix)))]
    PtyMessage(ReaderMessage),
    Deadline,
}

#[cfg(unix)]
#[derive(Default)]
struct DirectInput {
    state: Mutex<PtyWriteState>,
    echo: AtomicU64,
}

#[cfg(unix)]
#[derive(Default)]
struct PtyWriteState {
    open: bool,
    fd: Option<filedescriptor::FileDescriptor>,
    pending: Vec<u8>,
    offset: usize,
}

#[cfg(unix)]
enum DirectWrite {
    Closed,
    Written,
    Queued,
}

#[cfg(unix)]
impl DirectInput {
    fn close(&self) {
        self.state.lock().open = false;
    }

    fn reopen(&self, ready: impl FnOnce() -> bool) {
        let mut state = self.state.lock();
        state.open = state.fd.is_some() && !state.has_pending() && ready();
    }

    #[cfg(test)]
    fn is_open(&self) -> bool {
        self.state.lock().open
    }

    fn take_echo(&self) -> Option<Instant> {
        if self.echo.load(Ordering::Acquire) == 0 {
            return None;
        }
        echo_note_instant(self.echo.swap(0, Ordering::AcqRel))
    }

    fn echo_noted(&self) -> bool {
        echo_note_instant(self.echo.load(Ordering::Acquire)).is_some()
    }

    fn write(&self, bytes: &[u8]) -> DirectWrite {
        let mut state = self.state.lock();
        if !state.open || state.has_pending() {
            return DirectWrite::Closed;
        }
        let written = u64::try_from(OUTPUT_CLOCK.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.echo
            .store(written.saturating_add(1), Ordering::Release);
        state.compact_for(bytes.len());
        state.pending.extend_from_slice(bytes);
        if state.flush_pending().is_err() || state.has_pending() {
            state.open = false;
            return DirectWrite::Queued;
        }
        DirectWrite::Written
    }
}

#[cfg(unix)]
fn echo_note_instant(note: u64) -> Option<Instant> {
    let written = OUTPUT_CLOCK.checked_add(Duration::from_nanos(note.checked_sub(1)?))?;
    (written.elapsed() < ECHO_WINDOW).then_some(written)
}

#[cfg(unix)]
impl PtyWriteState {
    fn has_pending(&self) -> bool {
        self.offset < self.pending.len()
    }

    fn flush_pending(&mut self) -> std::io::Result<()> {
        let mut budget = PTY_WRITE_BUDGET_BYTES;
        while self.has_pending() && budget != 0 {
            let Some(fd) = &self.fd else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "PTY writer is closed",
                ));
            };
            let end = self.offset.saturating_add(budget).min(self.pending.len());
            match rustix::io::write(fd, &self.pending[self.offset..end]) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "PTY writer accepted no bytes",
                    ));
                }
                Ok(written) => {
                    self.offset = self.offset.saturating_add(written);
                    budget = budget.saturating_sub(written);
                }
                Err(rustix::io::Errno::AGAIN) => return Ok(()),
                Err(rustix::io::Errno::INTR) => {}
                Err(errno) => return Err(std::io::Error::from(errno)),
            }
        }
        if !self.has_pending() {
            self.pending.clear();
            self.offset = 0;
            if self.pending.capacity() > PTY_WRITE_RETAIN_BYTES {
                self.pending = Vec::new();
            }
        }
        Ok(())
    }

    fn compact_for(&mut self, additional: usize) {
        if self.pending.capacity().saturating_sub(self.pending.len()) < additional
            && self.offset != 0
        {
            let queued = self.pending.len().saturating_sub(self.offset);
            self.pending.copy_within(self.offset.., 0);
            self.pending.truncate(queued);
            self.offset = 0;
        }
    }
}

#[cfg(unix)]
struct PtyWriter {
    direct: Arc<DirectInput>,
}

#[cfg(unix)]
impl PtyWriter {
    #[cfg(test)]
    fn new(fd: filedescriptor::FileDescriptor) -> Self {
        Self::attach(Arc::default(), fd)
    }

    fn attach(direct: Arc<DirectInput>, fd: filedescriptor::FileDescriptor) -> Self {
        {
            let mut state = direct.state.lock();
            state.open = false;
            state.fd = Some(fd);
        }
        Self { direct }
    }

    fn has_pending(&self) -> bool {
        self.direct.state.lock().has_pending()
    }

    #[cfg(test)]
    fn queued_bytes(&self) -> usize {
        let state = self.direct.state.lock();
        state.pending.len().saturating_sub(state.offset)
    }

    #[cfg(test)]
    fn pending_capacity(&self) -> usize {
        self.direct.state.lock().pending.capacity()
    }

    fn flush_pending(&mut self) -> std::io::Result<()> {
        self.direct.state.lock().flush_pending()
    }
}

#[cfg(unix)]
impl Drop for PtyWriter {
    fn drop(&mut self) {
        let mut state = self.direct.state.lock();
        state.open = false;
        state.fd = None;
    }
}

#[cfg(unix)]
impl Write for PtyWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = self.direct.state.lock();
        state.compact_for(buf.len());
        state.pending.extend_from_slice(buf);
        state.flush_pending()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_pending()
    }
}

#[cfg(all(test, unix))]
#[path = "session/direct_input_echoin_tests.rs"]
mod direct_input_echoin_tests;

#[cfg(all(unix, not(target_os = "linux")))]
fn wait_for_wake(
    control_rx: &Receiver<Command>,
    input_rx: Option<&Receiver<QueuedInput>>,
    search_results: &Receiver<SearchResults>,
    mut child: Option<&mut ChildExitWatch>,
    pty: Option<&filedescriptor::FileDescriptor>,
    wake_rx: &std::os::fd::OwnedFd,
    timeout: Duration,
) -> Result<Wake, WorkerError> {
    use crossbeam_channel::TryRecvError;
    use rustix::event::{PollFd, PollFlags};

    fn check_channels(
        control_rx: &Receiver<Command>,
        input_rx: Option<&Receiver<QueuedInput>>,
        search_results: &Receiver<SearchResults>,
    ) -> Result<Option<Wake>, WorkerError> {
        match control_rx.try_recv() {
            Ok(command) => return Ok(Some(Wake::Command(command))),
            Err(TryRecvError::Disconnected) => return Ok(Some(Wake::CommandsClosed)),
            Err(TryRecvError::Empty) => {}
        }
        if let Some(input_rx) = input_rx {
            match input_rx.try_recv() {
                Ok(command) => return Ok(Some(Wake::Input(command))),
                Err(TryRecvError::Disconnected | TryRecvError::Empty) => {}
            }
        }
        match search_results.try_recv() {
            Ok(result) => Ok(Some(Wake::Search(result))),
            Err(TryRecvError::Disconnected) => Err(WorkerError::Thread(
                "terminal search worker stopped".to_owned(),
            )),
            Err(TryRecvError::Empty) => Ok(None),
        }
    }

    if let Some(wake) = check_channels(control_rx, input_rx, search_results)? {
        return Ok(wake);
    }
    if let Some(status) = child.as_deref_mut().and_then(ChildExitWatch::take_ready) {
        return Ok(Wake::ChildExit(status));
    }

    let timespec = rustix::event::Timespec::try_from(timeout.min(Duration::from_hours(1)))
        .expect("a bounded timeout fits in a timespec");
    let readable = PollFlags::IN | PollFlags::HUP | PollFlags::ERR;
    let (pty_ready, wake_ready, child_ready) = {
        let mut fds = SmallVec::<[PollFd<'_>; 3]>::new();
        fds.push(PollFd::new(wake_rx, PollFlags::IN));
        let pty_index = pty.map(|pty| {
            fds.push(PollFd::new(pty, PollFlags::IN));
            fds.len() - 1
        });
        let child_index = child
            .as_deref()
            .and_then(ChildExitWatch::poll_fd)
            .map(|kqueue| {
                fds.push(PollFd::new(kqueue, PollFlags::IN));
                fds.len() - 1
            });
        match rustix::event::poll(&mut fds, Some(&timespec)) {
            Ok(_) => {
                let ready = |index: Option<usize>| {
                    index.is_some_and(|index| fds[index].revents().intersects(readable))
                };
                (ready(pty_index), ready(Some(0)), ready(child_index))
            }
            Err(rustix::io::Errno::INTR) => (false, false, false),
            Err(error) => return Err(WorkerError::Io(error.into())),
        }
    };
    if wake_ready {
        drain_wake_pipe(wake_rx)?;
        if let Some(wake) = check_channels(control_rx, input_rx, search_results)? {
            return Ok(wake);
        }
    }
    if child_ready && let Some(status) = child.and_then(ChildExitWatch::on_readable) {
        return Ok(Wake::ChildExit(status));
    }
    if pty_ready {
        return Ok(Wake::PtyReadable);
    }
    Ok(Wake::Deadline)
}

#[cfg(unix)]
const PANE_ENVIRONMENT_KEYS: [&str; 6] = [
    "TERM",
    "COLORTERM",
    "CLAUDE_CODE_TMUX_TRUECOLOR",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "SHELL",
];

#[cfg(unix)]
struct UnixChildKiller(Option<u32>);

#[cfg(unix)]
impl UnixChildKiller {
    fn kill(&mut self) -> std::io::Result<()> {
        let Some(pid) = self
            .0
            .and_then(|pid| i32::try_from(pid).ok())
            .and_then(rustix::process::Pid::from_raw)
        else {
            return Ok(());
        };
        rustix::process::kill_process(pid, rustix::process::Signal::HUP).map_err(Into::into)
    }
}

#[cfg(unix)]
fn exit_status_from_wait(status: rustix::process::WaitStatus) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from(std::process::ExitStatus::from_raw(status.as_raw()))
}

#[cfg(unix)]
fn reap_child(pid: rustix::process::Pid, block: bool) -> Option<std::io::Result<ExitStatus>> {
    let options = if block {
        rustix::process::WaitOptions::empty()
    } else {
        rustix::process::WaitOptions::NOHANG
    };
    loop {
        match rustix::process::waitpid(Some(pid), options) {
            Ok(Some((_, status))) => return Some(Ok(exit_status_from_wait(status))),
            Ok(None) => return None,
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Some(Err(error.into())),
        }
    }
}

#[cfg(unix)]
fn child_pid(process_id: Option<u32>) -> Result<rustix::process::Pid, WorkerError> {
    process_id
        .and_then(|pid| i32::try_from(pid).ok())
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| WorkerError::Spawn("the spawned child has no process id".to_owned()))
}

#[cfg(all(unix, not(target_os = "linux")))]
struct ChildExitWatch {
    pid: rustix::process::Pid,
    kqueue: Option<std::os::fd::OwnedFd>,
    ready: Option<std::io::Result<ExitStatus>>,
    reaped: bool,
    #[cfg(target_os = "macos")]
    retry: Option<Instant>,
}

#[cfg(all(unix, not(target_os = "linux")))]
impl ChildExitWatch {
    #[cfg(target_os = "macos")]
    fn for_shard(process_id: Option<u32>) -> Result<Self, WorkerError> {
        let pid = child_pid(process_id)?;
        let ready = reap_child(pid, false);
        Ok(Self {
            pid,
            kqueue: None,
            reaped: ready.is_some(),
            ready,
            retry: None,
        })
    }

    fn new(process_id: Option<u32>) -> Result<Self, WorkerError> {
        use rustix::event::kqueue::{Event, EventFilter, EventFlags, ProcessEvents};

        let pid = child_pid(process_id)?;
        let kqueue =
            rustix::event::kqueue::kqueue().map_err(|error| WorkerError::Io(error.into()))?;
        let change = [Event::new(
            EventFilter::Proc {
                pid,
                flags: ProcessEvents::EXIT,
            },
            EventFlags::ADD | EventFlags::ONESHOT,
            std::ptr::null_mut(),
        )];
        #[allow(
            unsafe_code,
            reason = "a process filter names no descriptor that could close under the kqueue"
        )]
        let registered = unsafe {
            rustix::event::kqueue::kevent(&kqueue, &change, &mut [] as &mut [Event; 0], None)
        };
        match registered {
            Ok(_) => {
                let ready = reap_child(pid, false);
                Ok(Self {
                    pid,
                    kqueue: ready.is_none().then_some(kqueue),
                    reaped: ready.is_some(),
                    ready,
                    #[cfg(target_os = "macos")]
                    retry: None,
                })
            }
            Err(rustix::io::Errno::SRCH) => {
                let ready = reap_child(pid, false).or_else(|| reap_child(pid, true));
                Ok(Self {
                    pid,
                    kqueue: None,
                    reaped: ready.is_some(),
                    ready,
                    #[cfg(target_os = "macos")]
                    retry: None,
                })
            }
            Err(error) => Err(WorkerError::Io(error.into())),
        }
    }

    fn poll_fd(&self) -> Option<&std::os::fd::OwnedFd> {
        self.kqueue.as_ref()
    }

    fn take_ready(&mut self) -> Option<std::io::Result<ExitStatus>> {
        self.ready.take()
    }

    fn on_readable(&mut self) -> Option<std::io::Result<ExitStatus>> {
        use rustix::event::kqueue::Event;

        let exited = self.kqueue.as_ref().is_some_and(|kqueue| {
            let mut events = Vec::<Event>::with_capacity(1);
            #[allow(
                unsafe_code,
                reason = "reading events registers nothing, so no descriptor can dangle"
            )]
            let read = unsafe {
                rustix::event::kqueue::kevent(kqueue, &[], &mut events, Some(Duration::ZERO))
            };
            read.is_ok_and(|count| count > 0)
        });
        let Some(status) = reap_child(self.pid, exited) else {
            #[cfg(target_os = "macos")]
            if self.kqueue.is_none() {
                self.retry = Some(Instant::now() + SHARD_CHILD_RETRY);
            }
            return None;
        };
        self.kqueue = None;
        self.reaped = true;
        Some(status)
    }

    #[cfg(target_os = "macos")]
    fn retry_due(&mut self) -> Option<std::io::Result<ExitStatus>> {
        if self.retry.is_none_or(|due| due > Instant::now()) {
            return None;
        }
        self.retry = None;
        let status = reap_child(self.pid, false)?;
        self.reaped = true;
        Some(status)
    }

    fn wait_timeout(&mut self, timeout: Duration) -> Option<std::io::Result<ExitStatus>> {
        use rustix::event::{PollFd, PollFlags};

        if let Some(status) = self.ready.take() {
            return Some(status);
        }
        self.kqueue.as_ref()?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = reap_child(self.pid, false) {
                self.kqueue = None;
                self.reaped = true;
                return Some(status);
            }
            let kqueue = self.kqueue.as_ref()?;
            let remaining = deadline.checked_duration_since(Instant::now())?;
            let timespec = rustix::event::Timespec::try_from(remaining)
                .expect("a bounded timeout fits in a timespec");
            let mut fds = [PollFd::new(kqueue, PollFlags::IN)];
            match rustix::event::poll(&mut fds, Some(&timespec)) {
                Ok(0) => return None,
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(_) => return None,
            }
            if let Some(status) = self.on_readable() {
                return Some(status);
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
impl Drop for ChildExitWatch {
    fn drop(&mut self) {
        if self.reaped || reap_child(self.pid, false).is_some() {
            return;
        }
        let pid = self.pid;
        if let Err(error) = thread::Builder::new()
            .name("zz-child-reap".into())
            .spawn(move || {
                let _ = reap_child(pid, true);
            })
        {
            log::warn!(
                "could not wait for pane child {}: {error}",
                pid.as_raw_nonzero()
            );
        }
    }
}

#[cfg(target_os = "linux")]
struct LinuxChildWatch {
    pid: rustix::process::Pid,
    pidfd: std::os::fd::OwnedFd,
    exit: Sender<std::io::Result<ExitStatus>>,
    wake: ActorWake,
    reaped: Cell<bool>,
}

#[cfg(target_os = "linux")]
impl LinuxChildWatch {
    fn reap(&self) -> bool {
        if self.reaped.get() {
            return true;
        }
        let Some(status) = reap_child(self.pid, false) else {
            return false;
        };
        self.reaped.set(true);
        let _ = self.exit.try_send(status);
        self.wake.notify();
        true
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxChildWatch {
    fn drop(&mut self) {
        if self.reap() {
            return;
        }
        let pid = self.pid;
        if let Err(error) = thread::Builder::new()
            .name("zz-child-reap".into())
            .spawn(move || {
                let _ = reap_child(pid, true);
            })
        {
            log::warn!(
                "could not wait for pane child {}: {error}",
                pid.as_raw_nonzero()
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn watch_child_linux(
    process_id: Option<u32>,
    exit: Sender<std::io::Result<ExitStatus>>,
    wake: ActorWake,
) -> Result<Option<LinuxChildWatch>, WorkerError> {
    let pid = child_pid(process_id)?;
    if let Ok(pidfd) = rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()) {
        return Ok(Some(LinuxChildWatch {
            pid,
            pidfd,
            exit,
            wake,
            reaped: Cell::new(false),
        }));
    }
    thread::Builder::new()
        .name("zz-child-wait".into())
        .spawn(move || {
            let status = reap_child(pid, true).unwrap_or_else(|| {
                Err(std::io::Error::other("the pane child was reaped elsewhere"))
            });
            let _ = exit.try_send(status);
            wake.notify();
        })
        .map_err(WorkerError::Io)?;
    Ok(None)
}

#[cfg(unix)]
fn drain_wake_pipe(wake_rx: &std::os::fd::OwnedFd) -> Result<(), WorkerError> {
    let mut drained = [0_u8; 64];
    loop {
        match rustix::io::read(wake_rx, &mut drained) {
            Ok(0) | Err(rustix::io::Errno::AGAIN) => return Ok(()),
            Ok(read) if read < drained.len() => return Ok(()),
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(WorkerError::Io(error.into())),
        }
    }
}

#[cfg(not(unix))]
fn wait_for_wake(
    control_rx: &Receiver<Command>,
    input_rx: Option<&Receiver<QueuedInput>>,
    search_results: &Receiver<SearchResults>,
    child_exit: &Receiver<std::io::Result<ExitStatus>>,
    output_rx: &Receiver<ReaderMessage>,
    timeout: Duration,
) -> Result<Wake, WorkerError> {
    if let Some(input_rx) = input_rx {
        crossbeam_channel::select_biased! {
            recv(control_rx) -> message => Ok(message.map_or(Wake::CommandsClosed, Wake::Command)),
            recv(input_rx) -> message => message.map(Wake::Input).map_err(|_| {
                WorkerError::Thread("terminal input channel stopped".to_owned())
            }),
            recv(search_results) -> result => result.map(Wake::Search).map_err(|_| {
                WorkerError::Thread("terminal search worker stopped".to_owned())
            }),
            recv(output_rx) -> message => Ok(Wake::PtyMessage(message.unwrap_or(ReaderMessage::Eof))),
            recv(child_exit) -> status => status.map(Wake::ChildExit).map_err(|_| {
                WorkerError::Thread("terminal child waiter stopped".to_owned())
            }),
            default(timeout) => Ok(Wake::Deadline),
        }
    } else {
        crossbeam_channel::select_biased! {
            recv(control_rx) -> message => Ok(message.map_or(Wake::CommandsClosed, Wake::Command)),
            recv(search_results) -> result => result.map(Wake::Search).map_err(|_| {
                WorkerError::Thread("terminal search worker stopped".to_owned())
            }),
            recv(output_rx) -> message => Ok(Wake::PtyMessage(message.unwrap_or(ReaderMessage::Eof))),
            recv(child_exit) -> status => status.map(Wake::ChildExit).map_err(|_| {
                WorkerError::Thread("terminal child waiter stopped".to_owned())
            }),
            default(timeout) => Ok(Wake::Deadline),
        }
    }
}

#[cfg(target_os = "linux")]
fn pty_gather_enabled() -> bool {
    std::env::var_os("ZZ_PTY_GATHER").is_none_or(|value| value != "0")
}

#[cfg(target_os = "linux")]
#[derive(Clone)]
struct PtyGather {
    panes: Arc<GatherSender>,
    thread: Arc<thread::JoinHandle<()>>,
}

#[cfg(target_os = "linux")]
struct GatherSender {
    panes: Option<Sender<GatherPane>>,
    wake: ActorWake,
}

#[cfg(target_os = "linux")]
impl GatherSender {
    fn new(wake: ActorWake) -> (Self, Receiver<GatherPane>) {
        let (panes, receiver) = crossbeam_channel::unbounded();
        (
            Self {
                panes: Some(panes),
                wake,
            },
            receiver,
        )
    }

    fn send(&self, pane: GatherPane, back: &ActorWake) -> Result<(), GatherPane> {
        pane.source.slot.move_to(&self.wake);
        let sent = match &self.panes {
            Some(panes) => panes
                .send(pane)
                .map_err(crossbeam_channel::SendError::into_inner),
            None => Err(pane),
        };
        match &sent {
            Ok(()) => self.wake.notify(),
            Err(pane) => pane.source.slot.move_to(back),
        }
        sent
    }

    fn close(&mut self) {
        if self.panes.take().is_some() {
            self.wake.notify();
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for GatherSender {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(target_os = "linux")]
struct GatherSource {
    fd: filedescriptor::FileDescriptor,
    output: Sender<ReaderMessage>,
    recycled: Receiver<Vec<u8>>,
    wake: ActorWake,
    slot: Arc<GatherSlot>,
}

#[cfg(target_os = "linux")]
struct GatherSlot {
    starved: AtomicBool,
    closed: AtomicBool,
    owner: Mutex<ActorWake>,
    home: Mutex<Option<GatherPane>>,
}

#[cfg(target_os = "linux")]
impl GatherSlot {
    fn notify(&self) {
        self.owner.lock().notify();
    }

    fn move_to(&self, owner: &ActorWake) {
        *self.owner.lock() = owner.clone();
    }
}

#[cfg(target_os = "linux")]
struct GatherLease {
    slot: Arc<GatherSlot>,
}

#[cfg(target_os = "linux")]
struct GatherWake {
    rx: std::os::fd::OwnedFd,
    pending: Arc<AtomicBool>,
    wake: ActorWake,
}

#[cfg(target_os = "linux")]
impl GatherWake {
    fn new() -> Result<Self, WorkerError> {
        let (rx, pipe) =
            configured_actor_wake_pipe().map_err(|error| WorkerError::Io(error.into()))?;
        let pending = Arc::new(AtomicBool::new(false));
        let wake = ActorWake {
            ready: None,
            pipe: Some(Arc::new(pipe)),
            pending: Some(Arc::clone(&pending)),
        };
        Ok(Self { rx, pending, wake })
    }
}

#[cfg(target_os = "linux")]
impl PtyGather {
    fn start(name: String) -> Result<Self, WorkerError> {
        let wake = GatherWake::new()?;
        let (panes, incoming) = GatherSender::new(wake.wake.clone());
        let thread = thread::Builder::new()
            .name(name.clone())
            .spawn(move || Gather::new(name, wake, incoming, GatherRole::Home).run())
            .map_err(WorkerError::Io)?;
        Ok(Self {
            panes: Arc::new(panes),
            thread: Arc::new(thread),
        })
    }

    fn stopped(&self) -> bool {
        self.thread.is_finished()
    }

    fn lend(&self, mut pane: GatherPane) -> Result<(), GatherPane> {
        pane.filled = Some(Instant::now());
        pane.sent = pane.filled;
        let back = pane.source.wake.clone();
        self.panes.send(pane, &back)
    }
}

#[cfg(target_os = "linux")]
struct DirectReader {
    gather: Option<PtyGather>,
    pane: Option<GatherPane>,
    last: Instant,
    bytes: usize,
}

#[cfg(target_os = "linux")]
impl DirectReader {
    fn new(gather: Option<PtyGather>, pane: GatherPane) -> (Self, Vec<u8>) {
        let mut reader = Self {
            gather,
            pane: None,
            last: Instant::now(),
            bytes: 0,
        };
        let buffer = reader.park(pane);
        (reader, buffer)
    }

    fn fd(&self) -> Option<&filedescriptor::FileDescriptor> {
        self.pane.as_ref().map(|pane| &pane.source.fd)
    }

    fn park(&mut self, mut pane: GatherPane) -> Vec<u8> {
        let buffer = pane
            .buffer
            .take()
            .or_else(|| pane.source.recycled.try_recv().ok())
            .unwrap_or_else(|| vec![0_u8; PTY_READ_BUFFER_BYTES]);
        self.pane = Some(pane);
        self.bytes = 0;
        buffer
    }

    fn lend_if_busy(&mut self, read: usize, buffer: &mut Vec<u8>, buffers: &BufferReturn) -> bool {
        if self.gather.is_none() {
            return false;
        }
        let now = Instant::now();
        if now.saturating_duration_since(self.last) >= PTY_GATHER_LEND_BUSY {
            self.bytes = 0;
        }
        self.last = now;
        self.bytes += read;
        if self.bytes < PTY_READ_BUFFER_BYTES {
            return false;
        }
        let (Some(gather), Some(pane)) = (&self.gather, self.pane.take()) else {
            return false;
        };
        buffers.give(std::mem::take(buffer));
        let Err(pane) = gather.lend(pane) else {
            return true;
        };
        *buffer = self.park(pane);
        false
    }
}

#[cfg(target_os = "linux")]
impl GatherLease {
    fn take_home(&self) -> Option<GatherPane> {
        self.slot.home.lock().take()
    }

    fn returned(&self) {
        std::sync::atomic::fence(Ordering::SeqCst);
        if self.slot.starved.load(Ordering::Relaxed)
            && self.slot.starved.swap(false, Ordering::AcqRel)
        {
            self.slot.notify();
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for GatherLease {
    fn drop(&mut self) {
        let parked = {
            let mut home = self.slot.home.lock();
            self.slot.closed.store(true, Ordering::Release);
            home.take()
        };
        drop(parked);
        self.slot.notify();
    }
}

#[cfg(any(target_os = "linux", not(unix)))]
struct BufferReturn {
    buffers: Sender<Vec<u8>>,
    #[cfg(target_os = "linux")]
    gather: GatherLease,
}

#[cfg(any(target_os = "linux", not(unix)))]
impl BufferReturn {
    fn give(&self, buffer: Vec<u8>) {
        let _ = self.buffers.try_send(buffer);
        #[cfg(target_os = "linux")]
        self.gather.returned();
    }
}

#[cfg(target_os = "linux")]
struct GatherPane {
    source: GatherSource,
    buffer: Option<Vec<u8>>,
    length: usize,
    spins: u32,
    active: bool,
    waiting: bool,
    done: bool,
    filled: Option<Instant>,
    sent: Option<Instant>,
}

#[cfg(target_os = "linux")]
impl GatherPane {
    const fn new(source: GatherSource) -> Self {
        Self {
            source,
            buffer: None,
            length: 0,
            spins: 0,
            active: false,
            waiting: false,
            done: false,
            filled: None,
            sent: None,
        }
    }

    fn open(
        fd: filedescriptor::FileDescriptor,
        output: Sender<ReaderMessage>,
        recycled: Receiver<Vec<u8>>,
        wake: ActorWake,
    ) -> (Self, GatherLease) {
        let slot = Arc::new(GatherSlot {
            starved: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            owner: Mutex::new(wake.clone()),
            home: Mutex::new(None),
        });
        (
            Self::new(GatherSource {
                fd,
                output,
                recycled,
                wake,
                slot: Arc::clone(&slot),
            }),
            GatherLease { slot },
        )
    }

    fn go_home(mut self) -> Result<(), Self> {
        self.send();
        if self.done || self.source.output.is_full() {
            return Err(self);
        }
        self.active = false;
        self.waiting = false;
        self.spins = 0;
        let slot = Arc::clone(&self.source.slot);
        let output = self.source.output.clone();
        let wake = self.source.wake.clone();
        {
            let mut home = slot.home.lock();
            if slot.closed.load(Ordering::Acquire) {
                return Err(self);
            }
            slot.move_to(&wake);
            *home = Some(self);
        }
        if output.try_send(ReaderMessage::Home).is_ok() {
            wake.notify();
        }
        Ok(())
    }

    fn has_buffer(&mut self) -> bool {
        if self.buffer.is_none() {
            self.buffer = self.source.recycled.try_recv().ok().or_else(|| {
                self.source.slot.starved.store(true, Ordering::Relaxed);
                std::sync::atomic::fence(Ordering::SeqCst);
                let buffer = self.source.recycled.try_recv().ok();
                if buffer.is_some() {
                    self.source.slot.starved.store(false, Ordering::Relaxed);
                }
                buffer
            });
        }
        self.buffer.is_some()
    }

    fn step(&mut self) -> bool {
        let Some(buffer) = self.buffer.as_mut() else {
            return false;
        };
        debug_assert_eq!(buffer.len(), PTY_READ_BUFFER_BYTES);
        match rustix::io::read(&self.source.fd, &mut buffer[self.length..]) {
            Ok(0) | Err(rustix::io::Errno::IO) => {}
            Ok(read) => {
                self.length += read;
                self.spins = 0;
                self.waiting = false;
                if self.length < buffer.len() {
                    return true;
                }
                self.filled = Some(Instant::now());
                self.send();
                return !self.done && self.has_buffer();
            }
            Err(rustix::io::Errno::INTR) => return true,
            Err(rustix::io::Errno::AGAIN) => {
                self.waiting = self.length >= PTY_BRIDGE_THRESHOLD_BYTES;
                if !self.waiting {
                    self.send();
                }
                return false;
            }
            Err(error) => log::debug!("Linux PTY gather stopped: {error}"),
        }
        self.send();
        if !self.done {
            let _ = self.source.output.try_send(ReaderMessage::Eof);
            self.source.wake.notify();
        }
        self.done = true;
        false
    }

    fn missed(&mut self) {
        if !self.waiting {
            return;
        }
        self.spins += 1;
        if self.spins > PTY_GATHER_BRIDGE_SPIN_MAX {
            self.waiting = false;
            self.spins = 0;
            self.send();
        }
    }

    fn send(&mut self) {
        if self.length == 0 {
            return;
        }
        let length = std::mem::take(&mut self.length);
        let Some(buffer) = self.buffer.take() else {
            return;
        };
        if self
            .source
            .output
            .try_send(ReaderMessage::Data { buffer, length })
            .is_ok()
        {
            self.sent = Some(Instant::now());
            self.source.wake.notify();
        } else {
            self.done = true;
        }
    }

    fn quiet_for(&self, now: Instant) -> Duration {
        self.filled.map_or(Duration::MAX, |filled| {
            now.saturating_duration_since(filled)
        })
    }

    fn released(&self) -> bool {
        self.done || self.source.slot.closed.load(Ordering::Acquire)
    }
}

#[cfg(target_os = "linux")]
impl Drop for GatherPane {
    fn drop(&mut self) {
        if !self.released() {
            let _ = self.source.output.try_send(ReaderMessage::Eof);
            self.source.wake.notify();
        }
    }
}

#[cfg(target_os = "linux")]
fn poll_gather(
    wake_rx: &std::os::fd::OwnedFd,
    panes: &mut [GatherPane],
    timeout: Option<&rustix::event::Timespec>,
) -> rustix::io::Result<(bool, SmallVec<[usize; 8]>)> {
    use rustix::event::{PollFd, PollFlags};

    let polled = panes
        .iter_mut()
        .enumerate()
        .filter(|(_, pane)| !pane.done && !pane.active)
        .filter_map(|(index, pane)| pane.has_buffer().then_some(index))
        .collect::<SmallVec<[usize; 8]>>();
    let mut fds = SmallVec::<[PollFd<'_>; 9]>::new();
    fds.push(PollFd::new(wake_rx, PollFlags::IN));
    fds.extend(
        polled
            .iter()
            .map(|&index| PollFd::new(&panes[index].source.fd, PollFlags::IN)),
    );
    match rustix::event::poll(&mut fds, timeout) {
        Ok(_) => {}
        Err(rustix::io::Errno::INTR) => return Ok((false, SmallVec::new())),
        Err(error) => return Err(error),
    }
    Ok((
        !fds[0].revents().is_empty(),
        polled
            .iter()
            .zip(&fds[1..])
            .filter(|(_, fd)| !fd.revents().is_empty())
            .map(|(&index, _)| index)
            .collect(),
    ))
}

#[cfg(target_os = "linux")]
fn read_gather_passes(panes: &mut [GatherPane]) -> bool {
    let mut filled = false;
    for _ in 0..PTY_GATHER_ADMIT_PASSES {
        let mut active = false;
        for pane in panes.iter_mut().filter(|pane| pane.active) {
            let before = pane.filled;
            pane.active = pane.step();
            filled |= pane.filled != before;
            active |= pane.active;
        }
        if !active {
            break;
        }
    }
    filled
}

#[cfg(target_os = "linux")]
enum GatherRole {
    Home,
    Lent(GatherSender),
    Orphaned,
}

#[cfg(target_os = "linux")]
struct GatherLender {
    give: GatherSender,
    returns: Receiver<GatherPane>,
    idle: Option<Instant>,
}

#[cfg(target_os = "linux")]
struct Gather {
    name: String,
    wake: GatherWake,
    panes: Vec<GatherPane>,
    incoming: Option<Receiver<GatherPane>>,
    lent: Vec<GatherLender>,
    role: GatherRole,
}

#[cfg(target_os = "linux")]
impl Gather {
    const fn new(
        name: String,
        wake: GatherWake,
        incoming: Receiver<GatherPane>,
        role: GatherRole,
    ) -> Self {
        Self {
            name,
            wake,
            panes: Vec::new(),
            incoming: Some(incoming),
            lent: Vec::new(),
            role,
        }
    }

    fn run(mut self) {
        let now = rustix::event::Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        loop {
            self.accept();
            self.release();
            let home = match self.role {
                GatherRole::Home => self.hand_back(),
                _ => None,
            };
            let busy = self.panes.iter().any(|pane| pane.active || pane.waiting);
            let idle = match self.role {
                _ if busy => None,
                GatherRole::Home => self.retire().into_iter().chain(home).min(),
                GatherRole::Lent(_) => self.give_back(),
                GatherRole::Orphaned => None,
            };
            if self.panes.is_empty() && self.lent.is_empty() && self.incoming.is_none() {
                return;
            }
            let timeout = if busy {
                Some(now)
            } else {
                idle.map(|idle| rustix::event::Timespec::try_from(idle).unwrap_or(now))
            };
            let (woken, ready) = match poll_gather(&self.wake.rx, &mut self.panes, timeout.as_ref())
            {
                Ok(polled) => polled,
                Err(error) => {
                    log::error!("Linux PTY gather poll stopped: {error}");
                    return;
                }
            };
            if woken {
                let _ = drain_wake_pipe(&self.wake.rx);
                self.wake.pending.store(false, Ordering::SeqCst);
            }
            for index in ready {
                self.panes[index].active = true;
            }
            for pane in self.panes.iter_mut().filter(|pane| !pane.active) {
                pane.missed();
            }
            if read_gather_passes(&mut self.panes) && matches!(self.role, GatherRole::Home) {
                self.lend();
            }
        }
    }

    fn accept(&mut self) {
        if let Some(incoming) = &self.incoming {
            loop {
                match incoming.try_recv() {
                    Ok(pane) => self.panes.push(pane),
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        self.incoming = None;
                        break;
                    }
                }
            }
        }
        self.lent.retain_mut(|lender| {
            loop {
                match lender.returns.try_recv() {
                    Ok(pane) => {
                        self.panes.push(pane);
                        lender.idle = Some(Instant::now());
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => return true,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => return false,
                }
            }
        });
    }

    fn release(&mut self) {
        for pane in self.panes.extract_if(.., |pane| pane.released()) {
            if let GatherRole::Lent(home) = &self.role {
                let _ = home.send(pane, &self.wake.wake);
            }
        }
    }

    fn lend(&mut self) {
        let now = Instant::now();
        let mut kept = false;
        let mut index = 0;
        while index < self.panes.len() && self.lender_free() {
            if self.panes[index].done || self.panes[index].quiet_for(now) >= PTY_GATHER_LEND_BUSY {
                index += 1;
                continue;
            }
            if !kept {
                kept = true;
                index += 1;
                continue;
            }
            let pane = self.panes.swap_remove(index);
            if let Err(pane) = self.lend_pane(pane) {
                self.panes.push(pane);
                return;
            }
        }
    }

    fn lender_free(&self) -> bool {
        self.lent.len() < PTY_GATHER_LENT_MAX
            || self.lent.iter().any(|lender| lender.idle.is_some())
    }

    fn lend_pane(&mut self, pane: GatherPane) -> Result<(), GatherPane> {
        if self.lent.iter().all(|lender| lender.idle.is_none()) {
            match self.spawn_lent() {
                Ok(lender) => self.lent.push(lender),
                Err(error) => log::debug!("Linux PTY gather could not lend a busy pane: {error}"),
            }
        }
        let Some(lender) = self.lent.iter_mut().find(|lender| lender.idle.is_some()) else {
            return Err(pane);
        };
        lender.give.send(pane, &self.wake.wake)?;
        lender.idle = None;
        Ok(())
    }

    fn spawn_lent(&self) -> Result<GatherLender, WorkerError> {
        let wake = GatherWake::new()?;
        let (give, take) = GatherSender::new(wake.wake.clone());
        let (home, returns) = GatherSender::new(self.wake.wake.clone());
        let name = self.name.clone();
        thread::Builder::new()
            .name(name.clone())
            .spawn(move || Gather::new(name, wake, take, GatherRole::Lent(home)).run())
            .map_err(WorkerError::Io)?;
        Ok(GatherLender {
            give,
            returns,
            idle: Some(Instant::now()),
        })
    }

    fn retire(&mut self) -> Option<Duration> {
        if self.lent.is_empty() {
            return None;
        }
        let linger = if self.incoming.is_some() {
            PTY_GATHER_LEND_LINGER
        } else {
            Duration::ZERO
        };
        let now = Instant::now();
        for lender in &mut self.lent {
            if lender
                .idle
                .is_some_and(|idle| now.saturating_duration_since(idle) >= linger)
            {
                lender.give.close();
                lender.idle = None;
            }
        }
        self.lent
            .iter()
            .filter_map(|lender| lender.idle)
            .map(|idle| linger.saturating_sub(now.saturating_duration_since(idle)))
            .min()
    }

    fn hand_back(&mut self) -> Option<Duration> {
        let now = Instant::now();
        let mut wait = None::<Duration>;
        let mut index = 0;
        while index < self.panes.len() {
            let pane = &mut self.panes[index];
            if pane.done || pane.active || pane.waiting {
                index += 1;
                continue;
            }
            let quiet = pane
                .sent
                .map_or(Duration::MAX, |sent| now.saturating_duration_since(sent));
            if quiet < PTY_GATHER_LEND_IDLE {
                let left = PTY_GATHER_LEND_IDLE.saturating_sub(quiet);
                wait = Some(wait.map_or(left, |wait| wait.min(left)));
                index += 1;
                continue;
            }
            if !pane.has_buffer() {
                index += 1;
                continue;
            }
            if let Err(pane) = self.panes.swap_remove(index).go_home() {
                self.panes.push(pane);
                return Some(
                    wait.map_or(PTY_GATHER_LEND_BUSY, |wait| wait.min(PTY_GATHER_LEND_BUSY)),
                );
            }
        }
        wait
    }

    fn give_back(&mut self) -> Option<Duration> {
        let GatherRole::Lent(home) = &self.role else {
            return None;
        };
        let now = Instant::now();
        let mut wait = PTY_GATHER_LEND_IDLE;
        let mut index = 0;
        while index < self.panes.len() {
            let quiet = self.panes[index].quiet_for(now);
            if quiet < PTY_GATHER_LEND_IDLE {
                wait = wait.min(PTY_GATHER_LEND_IDLE.saturating_sub(quiet));
                index += 1;
                continue;
            }
            let pane = self.panes.swap_remove(index);
            if let Err(pane) = home.send(pane, &self.wake.wake) {
                self.panes.push(pane);
                self.role = GatherRole::Orphaned;
                return None;
            }
        }
        (!self.panes.is_empty()).then_some(wait)
    }
}

#[cfg(any(not(unix), test))]
#[allow(
    clippy::needless_pass_by_value,
    reason = "the reader thread owns the final sender so disconnect signals shutdown"
)]
fn read_pty(
    mut reader: Box<dyn Read + Send>,
    pending_output: Box<dyn Fn() -> usize + Send>,
    output: Sender<ReaderMessage>,
    recycled: Receiver<Vec<u8>>,
    wake: ActorWake,
) {
    let mut eof = false;
    while !eof {
        let Ok(mut buffer) = recycled.recv() else {
            return;
        };
        debug_assert_eq!(buffer.len(), PTY_READ_BUFFER_BYTES);
        let mut length = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                log::debug!("PTY reader reached EOF: {error}");
                break;
            }
        };
        let mut pending = pending_output();
        while length < buffer.len() && pending > 0 {
            let want = pending.min(buffer.len() - length);
            match reader.read(&mut buffer[length..length + want]) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(read) => {
                    length += read;
                    pending = pending.saturating_sub(read);
                    if pending == 0 {
                        pending = pending_output();
                    }
                }
                Err(error) => {
                    log::debug!("PTY reader reached EOF: {error}");
                    eof = true;
                    break;
                }
            }
        }
        if output.send(ReaderMessage::Data { buffer, length }).is_err() {
            return;
        }
        wake.notify();
    }
    let _ = output.send(ReaderMessage::Eof);
    wake.notify();
}

#[cfg(any(target_os = "linux", not(unix)))]
fn consume_pty_output(
    terminal: &mut Terminal<'_, '_>,
    passthrough: &mut PassthroughFilter,
    engine: &mut EngineOutput<'_>,
    publisher: &Publisher,
    buffer: Vec<u8>,
    length: usize,
    recycled: &BufferReturn,
) -> usize {
    log::trace!(
        target: "zz_terminal::diagnostics::pty",
        "read length={length} bytes={:?} text={:?}",
        &buffer[..length],
        String::from_utf8_lossy(&buffer[..length]),
    );
    if publisher.takes_output() {
        publisher.output(&Arc::from(&buffer[..length]));
    }
    let parsed = feed_pty_output(terminal, passthrough, engine, &buffer[..length]);
    recycled.give(buffer);
    parsed
}

struct EngineOutput<'a> {
    filter: &'a mut EngineFilter,
    knobs: EngineKnobs,
    renames: &'a mut Vec<String>,
    bar: &'a mut Option<ProgressBar>,
    last_command_status: &'a mut Option<CommandStatusUpdate>,
}

fn feed_pty_output(
    terminal: &mut Terminal<'_, '_>,
    passthrough: &mut PassthroughFilter,
    engine: &mut EngineOutput<'_>,
    bytes: &[u8],
) -> usize {
    let EngineOutput {
        filter,
        knobs,
        renames,
        bar,
        last_command_status,
    } = engine;
    if !bytes.is_empty() {
        filter.count_output();
    }
    passthrough.write(bytes, |unwrapped| {
        filter.write(
            unwrapped,
            *knobs,
            terminal,
            renames,
            bar,
            last_command_status,
        );
    })
}

#[cfg(any(target_os = "linux", not(unix), test))]
fn drain_pty_output_burst(
    output: &Receiver<ReaderMessage>,
    first_buffer: Vec<u8>,
    first_length: usize,
    limit_turn: bool,
    max_chunks: usize,
    mut consume: impl FnMut(Vec<u8>, usize),
) -> Option<ReaderMessage> {
    let started = limit_turn.then(Instant::now);
    let mut bytes = first_length;
    consume(first_buffer, first_length);

    for _ in 1..PTY_BUFFER_POOL_SIZE.min(max_chunks) {
        if limit_turn
            && (bytes >= PTY_DRAIN_TURN_BYTES
                || started.is_some_and(|started| started.elapsed() >= PTY_DRAIN_TURN_TIME))
        {
            break;
        }
        match output.try_recv() {
            Ok(ReaderMessage::Data { buffer, length }) => {
                bytes += length;
                consume(buffer, length);
            }
            Ok(message) => return Some(message),
            Err(_) => break,
        }
    }
    None
}

fn drain_effects(effects: &RefCell<PtyEffects>, writer: &mut dyn Write) -> Result<(), WorkerError> {
    let mut effects = effects.borrow_mut();
    if !effects.bytes.is_empty() {
        writer.write_all(&effects.bytes)?;
        effects.bytes.clear();
    }
    let overflowed = std::mem::take(&mut effects.overflowed);
    drop(effects);
    if overflowed {
        log::warn!("terminal PTY responses exceeded the {MAX_PTY_RESPONSE_BYTES}-byte limit");
    }
    writer.flush()?;
    Ok(())
}

#[cfg(unix)]
fn pty_effects_pending(effects: &RefCell<PtyEffects>) -> bool {
    let effects = effects.borrow();
    !effects.bytes.is_empty() || effects.overflowed
}

#[cfg(unix)]
fn drain_effects_if_writer_ready(
    effects: &RefCell<PtyEffects>,
    writer: &mut PtyWriter,
) -> Result<(), WorkerError> {
    if writer.has_pending() || !pty_effects_pending(effects) {
        return Ok(());
    }
    drain_effects(effects, writer)
}

#[cfg(unix)]
fn direct_key_bytes(input: &KeyInput) -> Option<&[u8]> {
    if !matches!(input.action, KeyAction::Press | KeyAction::Repeat)
        || input.modifiers != Modifiers::default()
    {
        return None;
    }
    match input.key {
        KeyCode::Character(_) => input
            .text
            .as_deref()
            .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
            .map(str::as_bytes),
        KeyCode::Enter if input.text.is_none() => Some(b"\r"),
        _ => None,
    }
}

fn encode_key(
    terminal: &Terminal<'_, '_>,
    encoder: &mut key::Encoder<'_>,
    event: &mut key::Event<'_>,
    input: KeyInput,
    erase_byte: Option<u8>,
    writer: &mut dyn Write,
    input_bytes: &mut Vec<u8>,
) -> Result<(), WorkerError> {
    if matches!(input.key, KeyCode::User(_)) {
        return Ok(());
    }
    let mut modifiers = key::Mods::empty();
    modifiers.set(key::Mods::SHIFT, input.modifiers.shift());
    modifiers.set(key::Mods::CTRL, input.modifiers.control());
    modifiers.set(key::Mods::ALT, input.modifiers.alt());
    modifiers.set(key::Mods::SUPER, input.modifiers.platform());

    event
        .set_action(match input.action {
            KeyAction::Press => key::Action::Press,
            KeyAction::Repeat => key::Action::Repeat,
            KeyAction::Release => key::Action::Release,
        })
        .set_key(ghostty_key(input.key))
        .set_mods(modifiers)
        .set_composing(false)
        .set_utf8(input.text)
        .set_unshifted_codepoint(input.unshifted_codepoint.unwrap_or('\0'));

    encoder.set_options_from_terminal(terminal);
    #[cfg(target_os = "macos")]
    encoder.set_macos_option_as_alt(key::OptionAsAlt::True);

    input_bytes.clear();
    encoder.encode_to_vec(event, input_bytes)?;
    if input.key == KeyCode::Backspace
        && input.modifiers == Modifiers::default()
        && input_bytes.as_slice() == [0x7f]
    {
        input_bytes.clear();
        input_bytes.extend(erase_byte);
    }
    if !input_bytes.is_empty() {
        writer.write_all(input_bytes)?;
        writer.flush()?;
    }
    Ok(())
}

fn ghostty_key(key: KeyCode) -> key::Key {
    match key {
        KeyCode::Character(character) => match character.to_ascii_lowercase() {
            '`' => key::Key::Backquote,
            '\\' => key::Key::Backslash,
            '[' => key::Key::BracketLeft,
            ']' => key::Key::BracketRight,
            ',' => key::Key::Comma,
            '0' => key::Key::Digit0,
            '1' => key::Key::Digit1,
            '2' => key::Key::Digit2,
            '3' => key::Key::Digit3,
            '4' => key::Key::Digit4,
            '5' => key::Key::Digit5,
            '6' => key::Key::Digit6,
            '7' => key::Key::Digit7,
            '8' => key::Key::Digit8,
            '9' => key::Key::Digit9,
            '=' => key::Key::Equal,
            'a' => key::Key::A,
            'b' => key::Key::B,
            'c' => key::Key::C,
            'd' => key::Key::D,
            'e' => key::Key::E,
            'f' => key::Key::F,
            'g' => key::Key::G,
            'h' => key::Key::H,
            'i' => key::Key::I,
            'j' => key::Key::J,
            'k' => key::Key::K,
            'l' => key::Key::L,
            'm' => key::Key::M,
            'n' => key::Key::N,
            'o' => key::Key::O,
            'p' => key::Key::P,
            'q' => key::Key::Q,
            'r' => key::Key::R,
            's' => key::Key::S,
            't' => key::Key::T,
            'u' => key::Key::U,
            'v' => key::Key::V,
            'w' => key::Key::W,
            'x' => key::Key::X,
            'y' => key::Key::Y,
            'z' => key::Key::Z,
            '-' => key::Key::Minus,
            '.' => key::Key::Period,
            '\'' => key::Key::Quote,
            ';' => key::Key::Semicolon,
            '/' => key::Key::Slash,
            ' ' => key::Key::Space,
            _ => key::Key::Unidentified,
        },
        KeyCode::Backspace => key::Key::Backspace,
        KeyCode::Enter => key::Key::Enter,
        KeyCode::Tab => key::Key::Tab,
        KeyCode::Escape => key::Key::Escape,
        KeyCode::Delete => key::Key::Delete,
        KeyCode::Insert => key::Key::Insert,
        KeyCode::Home => key::Key::Home,
        KeyCode::End => key::Key::End,
        KeyCode::PageUp => key::Key::PageUp,
        KeyCode::PageDown => key::Key::PageDown,
        KeyCode::ArrowUp => key::Key::ArrowUp,
        KeyCode::ArrowDown => key::Key::ArrowDown,
        KeyCode::ArrowLeft => key::Key::ArrowLeft,
        KeyCode::ArrowRight => key::Key::ArrowRight,
        KeyCode::Function(number) => match number {
            1 => key::Key::F1,
            2 => key::Key::F2,
            3 => key::Key::F3,
            4 => key::Key::F4,
            5 => key::Key::F5,
            6 => key::Key::F6,
            7 => key::Key::F7,
            8 => key::Key::F8,
            9 => key::Key::F9,
            10 => key::Key::F10,
            11 => key::Key::F11,
            12 => key::Key::F12,
            13 => key::Key::F13,
            14 => key::Key::F14,
            15 => key::Key::F15,
            16 => key::Key::F16,
            17 => key::Key::F17,
            18 => key::Key::F18,
            19 => key::Key::F19,
            20 => key::Key::F20,
            21 => key::Key::F21,
            22 => key::Key::F22,
            23 => key::Key::F23,
            24 => key::Key::F24,
            _ => key::Key::Unidentified,
        },
        KeyCode::Unidentified | KeyCode::User(_) => key::Key::Unidentified,
    }
}

const ECHO_WINDOW: Duration = Duration::from_millis(50);
const ECHO_PUBLISHES: u8 = 4;

#[derive(Default)]
struct EchoWindow {
    opened: Option<Instant>,
    publishes: u8,
}

impl EchoWindow {
    fn open(&mut self) {
        self.open_at(Instant::now());
    }

    fn open_at(&mut self, opened: Instant) {
        self.opened = Some(opened);
        self.publishes = ECHO_PUBLISHES;
    }

    fn due(&self) -> bool {
        self.publishes > 0
            && self
                .opened
                .is_some_and(|opened| opened.elapsed() < ECHO_WINDOW)
    }

    fn spend(&mut self) {
        self.publishes = self.publishes.saturating_sub(1);
    }
}

const COMPRESS_IDLE: Duration = Duration::from_secs(1);
const COMPRESS_STEP_BUDGET: Duration = Duration::from_millis(2);

#[derive(Default)]
struct IdleCompression {
    activity: Option<libghostty_vt::terminal::CompressionActivity>,
    due: Option<Instant>,
    unsupported: bool,
}

impl IdleCompression {
    fn enabled(&self) -> bool {
        !self.unsupported
    }

    fn observe(&mut self, terminal: &Terminal<'_, '_>, now: Instant) {
        if !self.enabled() {
            return;
        }
        let Ok(activity) = terminal.compression_activity() else {
            return;
        };
        if self.activity != Some(activity) {
            self.activity = Some(activity);
            self.due = Some(now + COMPRESS_IDLE);
        }
    }

    fn rearm(&mut self) {
        if self.enabled() {
            self.due = Some(Instant::now() + COMPRESS_IDLE);
        }
    }

    const fn due(&self) -> Option<Instant> {
        self.due
    }

    fn run(&mut self, terminal: &mut Terminal<'_, '_>) {
        use libghostty_vt::terminal::{CompressionMode, CompressionResult};

        if self.due.is_none_or(|due| Instant::now() < due) {
            return;
        }
        let started = Instant::now();
        loop {
            match terminal.compress(CompressionMode::Incremental) {
                Ok(CompressionResult::Pending) => {
                    if started.elapsed() >= COMPRESS_STEP_BUDGET {
                        self.due = Some(Instant::now());
                        break;
                    }
                }
                Ok(CompressionResult::Complete) => {
                    self.due = None;
                    break;
                }
                Ok(CompressionResult::Unsupported) => {
                    self.unsupported = true;
                    self.due = None;
                    break;
                }
                Err(error) => {
                    log::debug!("terminal history compression stopped: {error}");
                    self.due = None;
                    break;
                }
            }
        }
        log::trace!(
            target: "zz_terminal::diagnostics::compression",
            "compress step elapsed_us={} pending={}",
            started.elapsed().as_micros(),
            self.due.is_some(),
        );
        self.activity = terminal.compression_activity().ok();
    }
}

const UNWATCHED_SETTLE_QUIET: Duration = Duration::from_millis(100);
const UNWATCHED_SETTLE_MAX: Duration = Duration::from_secs(1);
const VIEWED_RENDER_RETAIN: Duration = Duration::from_secs(5);
const UNWATCHED_NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

struct RenderResources<'alloc> {
    state: RenderState<'alloc>,
    rows: RowIterator<'alloc>,
    cells: CellIterator<'alloc>,
}

impl<'alloc> RenderResources<'alloc> {
    fn new(terminal: &Terminal<'alloc, '_>) -> Result<Self, WorkerError> {
        let mut state = RenderState::new()?;
        state.set_clip(Clip {
            y: 0,
            rows: 1,
            cols: 1,
        })?;
        state.update(terminal)?.set_dirty(Dirty::Full)?;
        Ok(Self {
            state,
            rows: RowIterator::new()?,
            cells: CellIterator::new()?,
        })
    }
}

#[derive(Clone, Copy, Default)]
struct StreamState {
    stream: ViewStream,
    epoch: u64,
}

#[derive(PartialEq)]
struct SinkEdge {
    title: Arc<str>,
    working_directory: Option<Arc<str>>,
    status: SessionStatus,
}

impl SinkEdge {
    fn of(viewport: &TerminalViewport) -> Self {
        Self {
            title: Arc::clone(&viewport.presentation.title),
            working_directory: viewport.presentation.working_directory.clone(),
            status: viewport.status.clone(),
        }
    }
}

struct Frames<'alloc> {
    #[cfg(test)]
    snapshot_builds: usize,
    render: Option<RenderResources<'alloc>>,
    generations: ViewportGenerations,
    dictionary: ViewportDictionary,
    streams: HashMap<TerminalViewId, StreamState>,
    areas: HashMap<TerminalViewId, (u16, u16)>,
    idle_bound: Option<(u16, u16)>,
    preview: bool,
    force_fallback: bool,
    mode_views: HashSet<TerminalViewId>,
    published: HashSet<TerminalViewId>,
    unbuilt_since: Option<Instant>,
    last_unbuilt: Option<Instant>,
    last_notify: Option<Instant>,
    notify_owed: bool,
    synchronized_output_deadline: Option<Instant>,
    retain_render_until: Option<Instant>,
    watched: bool,
    last_settle: Option<Instant>,
    sink_edge: Option<SinkEdge>,
}

impl<'alloc> Frames<'alloc> {
    fn new(appearance: &TerminalAppearance) -> Result<Self, WorkerError> {
        Ok(Self {
            #[cfg(test)]
            snapshot_builds: 0,
            render: None,
            generations: ViewportGenerations::new()?,
            dictionary: ViewportDictionary {
                class_hints: ClassHints::new(appearance),
                ..ViewportDictionary::default()
            },
            streams: HashMap::new(),
            areas: HashMap::new(),
            idle_bound: None,
            preview: false,
            force_fallback: false,
            mode_views: HashSet::new(),
            published: HashSet::new(),
            unbuilt_since: None,
            last_unbuilt: None,
            last_notify: None,
            notify_owed: false,
            synchronized_output_deadline: None,
            retain_render_until: None,
            watched: false,
            last_settle: None,
            sink_edge: None,
        })
    }

    fn defer_synchronized_output(
        &mut self,
        terminal: &mut Terminal<'_, '_>,
        status: &SessionStatus,
    ) -> Result<bool, WorkerError> {
        if !terminal.mode(Mode::SYNC_OUTPUT)? {
            self.synchronized_output_deadline = None;
            return Ok(false);
        }
        let now = Instant::now();
        let deadline = *self
            .synchronized_output_deadline
            .get_or_insert(now + SYNCHRONIZED_OUTPUT_TIMEOUT);
        if matches!(status, SessionStatus::Running) && now < deadline {
            return Ok(true);
        }
        self.synchronized_output_deadline = None;
        terminal.set_mode(Mode::SYNC_OUTPUT, false)?;
        Ok(false)
    }

    fn unwatched(&self, active: &ActiveTerminalViews) -> bool {
        !self.needs_cells(active)
            && self.mode_views.is_empty()
            && active.values().all(|view| view.copy_mode.is_none())
    }

    fn streaming(&self, view: TerminalViewId) -> bool {
        self.streams
            .get(&view)
            .is_some_and(|state| state.stream.is_on())
    }

    fn epoch(&self, view: TerminalViewId) -> u64 {
        self.streams.get(&view).map_or(0, |state| state.epoch)
    }

    fn set_stream(&mut self, view: TerminalViewId, stream: ViewStream) -> bool {
        let restart = stream.is_on() && !self.published.contains(&view);
        let state = self.streams.entry(view).or_default();
        state.stream = stream;
        if restart {
            state.epoch = state.epoch.wrapping_add(1);
        }
        restart
    }

    fn shows(&self, view: TerminalViewId, active: &ActiveTerminalViews) -> bool {
        self.streaming(view)
            || self.published.contains(&view)
            || active
                .get(&view)
                .is_some_and(|state| state.copy_mode.is_some())
    }

    fn forget_view(&mut self, view: TerminalViewId) {
        self.streams.remove(&view);
        self.areas.remove(&view);
        self.mode_views.remove(&view);
        self.published.remove(&view);
    }

    fn frame_bound(&self, active: &ActiveTerminalViews) -> Option<(u16, u16)> {
        if self.preview {
            return None;
        }
        let mut views = active
            .keys()
            .filter(|view| self.streaming(**view))
            .peekable();
        if views.peek().is_none() {
            return self.idle_bound;
        }
        let mut bound = None;
        for view in views {
            let (columns, rows) = *self.areas.get(view)?;
            bound = Some(
                bound.map_or((columns, rows), |(most_columns, most_rows): (u16, u16)| {
                    (most_columns.max(columns), most_rows.max(rows))
                }),
            );
        }
        bound
    }

    fn frame_extent(
        &self,
        terminal: &Terminal<'_, '_>,
        active: &ActiveTerminalViews,
    ) -> Result<(u16, u16), WorkerError> {
        Ok(bounded_extent(
            (terminal.cols()?, terminal.rows()?),
            self.frame_bound(active),
        ))
    }

    fn resources(
        &mut self,
        terminal: &Terminal<'alloc, '_>,
    ) -> Result<&mut RenderResources<'alloc>, WorkerError> {
        if self.render.is_none() {
            self.render = Some(RenderResources::new(terminal)?);
        }
        Ok(self
            .render
            .as_mut()
            .expect("render resources were just created"))
    }

    fn reset_render(&mut self) {
        self.render = None;
    }

    fn mark_full_dirty(&mut self, terminal: &Terminal<'alloc, '_>) -> Result<(), WorkerError> {
        if let Some(render) = self.render.as_mut() {
            render.state.update(terminal)?.set_dirty(Dirty::Full)?;
        }
        Ok(())
    }

    fn snapshot(
        &mut self,
        terminal: &Terminal<'alloc, '_>,
        change: SnapshotChange,
        view: Option<&TerminalViewState>,
        status: SessionStatus,
        bound: Option<(u16, u16)>,
    ) -> Result<TerminalViewport, WorkerError> {
        #[cfg(test)]
        {
            self.snapshot_builds += 1;
        }
        if let Some(view) = view
            && let Some(copy_mode) = view.copy_mode.as_ref()
        {
            return Ok(copy_mode_snapshot(
                &mut self.generations,
                change,
                &mut self.dictionary,
                view,
                copy_mode,
                status,
                bound,
            ));
        }
        self.resources(terminal)?;
        let render = self.render.as_mut().expect("render resources exist");
        snapshot(
            terminal,
            &mut render.state,
            &mut render.rows,
            &mut render.cells,
            &mut self.generations,
            change,
            &mut self.dictionary,
            view,
            status,
            bound,
        )
    }

    fn needs_cells(&self, active: &ActiveTerminalViews) -> bool {
        self.preview || active.keys().any(|view| self.streaming(*view))
    }

    fn release_unused(&mut self, active: &ActiveTerminalViews) {
        if self.needs_cells(active) {
            self.retain_render_until = None;
            self.watched = true;
            return;
        }
        if std::mem::take(&mut self.watched) && self.render.is_some() {
            self.retain_render_until = Some(Instant::now() + VIEWED_RENDER_RETAIN);
            return;
        }
        if self
            .retain_render_until
            .is_some_and(|until| Instant::now() < until)
        {
            return;
        }
        self.retain_render_until = None;
        self.render = None;
        self.dictionary.release_pools();
    }

    fn rebuild_due(&self) -> Option<Instant> {
        let first = self.unbuilt_since?;
        let last = self.last_unbuilt.unwrap_or(first);
        Some((last + UNWATCHED_SETTLE_QUIET).min(first + UNWATCHED_SETTLE_MAX))
    }

    fn notify_due(&self) -> Option<Instant> {
        self.notify_owed
            .then(|| {
                self.last_notify
                    .map(|last| last + UNWATCHED_NOTIFY_INTERVAL)
            })
            .flatten()
    }

    fn settle_due(&self) -> Option<Instant> {
        [
            self.rebuild_due(),
            self.notify_due(),
            self.retain_render_until,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn admit_notify(&mut self, metadata_changed: bool) -> bool {
        let now = Instant::now();
        if metadata_changed
            || self
                .last_notify
                .is_none_or(|last| now >= last + UNWATCHED_NOTIFY_INTERVAL)
        {
            self.last_notify = Some(now);
            self.notify_owed = false;
            true
        } else {
            self.notify_owed = true;
            false
        }
    }
}

fn publish_active_views<'alloc: 'callbacks, 'callbacks>(
    terminal: &mut Terminal<'alloc, 'callbacks>,
    publisher: &Publisher,
    frames: &mut Frames<'alloc>,
    change: SnapshotChange,
    active: &mut ActiveTerminalViews,
    word_separators: &WordSeparators,
    status: SessionStatus,
) -> Result<(), WorkerError> {
    publish_views(
        terminal,
        publisher,
        frames,
        change,
        active,
        word_separators,
        status,
        true,
    )
}

#[cfg(test)]
#[path = "session/live_frames_tests.rs"]
mod live_frames_tests;

fn share_live_frame(left: &TerminalViewState, right: &TerminalViewState) -> bool {
    let live = |view: &TerminalViewState| {
        view.copy_mode.is_none()
            && view.selection.is_none()
            && matches!(view.viewport, ViewportAnchor::FollowBottom)
    };
    live(left)
        && live(right)
        && left.screen == right.screen
        && left.unseen_output == right.unseen_output
        && left.hover_link == right.hover_link
        && left.search.as_ref().map(|search| {
            (
                &search.matches,
                search.current,
                search.pending,
                search.invalid_pattern,
            )
        }) == right.search.as_ref().map(|search| {
            (
                &search.matches,
                search.current,
                search.pending,
                search.invalid_pattern,
            )
        })
}

#[allow(
    clippy::too_many_arguments,
    reason = "the publish entry point threads the actor's frame state and its notification choice"
)]
fn publish_views<'alloc: 'callbacks, 'callbacks>(
    terminal: &mut Terminal<'alloc, 'callbacks>,
    publisher: &Publisher,
    frames: &mut Frames<'alloc>,
    change: SnapshotChange,
    active: &mut ActiveTerminalViews,
    word_separators: &WordSeparators,
    status: SessionStatus,
    notify: bool,
) -> Result<(), WorkerError> {
    if frames.defer_synchronized_output(terminal, &status)? {
        return Ok(());
    }
    let force_fallback = std::mem::take(&mut frames.force_fallback);
    let bound = frames.frame_bound(active);
    let size = (terminal.cols()?, terminal.rows()?);
    let mut view_ids = active.keys().copied().collect::<Vec<_>>();
    view_ids.sort_by_key(|view| view.0);
    let mut viewports: Vec<(TerminalViewId, Arc<TerminalViewport>, Option<u64>)> =
        Vec::with_capacity(view_ids.len());
    let mut copy_facts = HashMap::new();
    let mut streamed_any = false;
    for view_id in view_ids {
        let streaming = frames.streaming(view_id);
        let view = active
            .get_mut(&view_id)
            .expect("active view id was collected from the same map");
        let in_mode = view.copy_mode.is_some();
        if let Some(facts) = view
            .copy_mode
            .as_deref()
            .map(|mode| copy_mode_facts(mode, view.search.as_deref(), word_separators))
        {
            copy_facts.insert(view_id, Arc::new(facts));
        }
        let left_mode = !in_mode && frames.mode_views.remove(&view_id);
        if !streaming && !in_mode && !left_mode {
            continue;
        }
        if in_mode {
            frames.mode_views.insert(view_id);
            terminal.set_selection(None)?;
        } else {
            restore_view_state(terminal, view, word_separators)?;
        }
        let view = active
            .get(&view_id)
            .expect("active view id was collected from the same map");
        let shared = viewports.iter().find(|(previous, _, _)| {
            share_live_frame(
                view,
                active.get(previous).expect("published view remains active"),
            )
        });
        let viewport = if let Some((_, viewport, _)) = shared {
            Arc::clone(viewport)
        } else {
            Arc::new(frames.snapshot(terminal, change, Some(view), status.clone(), bound)?)
        };
        streamed_any |= streaming;
        viewports.push((view_id, viewport, streaming.then(|| frames.epoch(view_id))));
    }
    if streamed_any {
        frames.idle_bound = bound;
    }
    let mut sunk = Vec::new();
    let frame_sink = publisher.frame_sink().filter(|_| streamed_any);
    let urgent = frame_sink.is_some_and(|sink| sink.deliver(&viewports, &mut sunk));
    let refreshed = if streamed_any || force_fallback || frames.preview {
        None
    } else {
        publisher.refresh_fallback(terminal, &mut frames.dictionary, &status, bound)?
    };
    let mut notify = notify;
    let fallback = if streamed_any {
        frames.unbuilt_since = None;
        frames.last_unbuilt = None;
        let all_sunk = !frames.preview && viewports.iter().all(|(view, _, _)| sunk.contains(view));
        if !all_sunk {
            frames.sink_edge = None;
        } else if notify {
            let edge = viewports
                .iter()
                .find(|(_, _, epoch)| epoch.is_some())
                .map(|(_, viewport, _)| SinkEdge::of(viewport));
            let changed = frames.sink_edge != edge;
            frames.sink_edge = edge;
            notify = frames.admit_notify(changed || urgent);
        }
        FallbackFrame::FirstStreamed
    } else if let Some((viewport, metadata_changed)) = refreshed {
        if matches!(change, SnapshotChange::Content) {
            let now = Instant::now();
            frames.unbuilt_since.get_or_insert(now);
            frames.last_unbuilt = Some(now);
        }
        notify = notify && frames.admit_notify(metadata_changed);
        FallbackFrame::Metadata(viewport)
    } else {
        if !active.is_empty() {
            terminal.set_selection(None)?;
            terminal.scroll_viewport(ScrollViewport::Bottom);
        }
        frames.unbuilt_since = None;
        frames.last_unbuilt = None;
        FallbackFrame::Built(frames.snapshot(terminal, change, None, status, bound)?)
    };
    frames.published.clear();
    frames
        .published
        .extend(viewports.iter().map(|(view, _, _)| *view));
    let notified = publisher.publish_frame(fallback, viewports, copy_facts, sunk, size, notify);
    if let Some(sink) = frame_sink {
        sink.published(notified);
    }
    frames.release_unused(active);
    Ok(())
}

#[cfg(test)]
#[path = "session/settle_tests.rs"]
mod settle_tests;

fn settle_unwatched<'alloc: 'callbacks, 'callbacks>(
    terminal: &mut Terminal<'alloc, 'callbacks>,
    publisher: &Publisher,
    frames: &mut Frames<'alloc>,
    active: &mut ActiveTerminalViews,
    word_separators: &WordSeparators,
    status: SessionStatus,
) -> Result<(), WorkerError> {
    let now = Instant::now();
    if frames.notify_due().is_some_and(|due| now >= due) {
        frames.notify_owed = false;
        frames.last_notify = Some(now);
        publisher.notify_latest();
    }
    if frames.rebuild_due().is_none_or(|due| now < due) {
        if frames.retain_render_until.is_some_and(|until| now >= until) {
            frames.release_unused(active);
        }
        return Ok(());
    }
    let horizon = UNWATCHED_SETTLE_MAX + UNWATCHED_SETTLE_QUIET;
    let recurring = frames
        .last_unbuilt
        .is_some_and(|last| now < last + UNWATCHED_SETTLE_QUIET)
        || frames.last_settle.is_some_and(|last| now < last + horizon);
    frames.last_settle = Some(now);
    frames.retain_render_until = frames
        .retain_render_until
        .max(recurring.then(|| now + horizon));
    frames.force_fallback = true;
    publish_views(
        terminal,
        publisher,
        frames,
        SnapshotChange::Content,
        active,
        word_separators,
        status,
        false,
    )?;
    frames.force_fallback = false;
    Ok(())
}

fn refresh_copy_revision(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    mode: &mut CopyModeState,
    mode_keys_vi: bool,
) -> Result<(), WorkerError> {
    let rows = u32::from(mode.revision.viewport_rows.saturating_sub(1));
    let follow = mode.viewport_offset == mode.revision.maximum_offset()
        && mode.cursor.y == mode.viewport_offset.saturating_add(rows);
    let offset_from_top = mode.viewport_offset;
    mode.revision = ModeRevision::capture(terminal)?;
    if follow {
        mode.viewport_offset = mode.revision.maximum_offset();
        mode.cursor = mode.revision.clamp_point(PointCoordinate {
            x: mode.cursor.x,
            y: mode
                .viewport_offset
                .saturating_add(u32::from(mode.revision.viewport_rows.saturating_sub(1))),
        });
        mode.cursor.x = copy_cursor_limit(&mode.revision, mode.cursor.y, mode_keys_vi, false);
    } else {
        mode.viewport_offset = offset_from_top.min(mode.revision.maximum_offset());
        mode.cursor = mode.revision.clamp_point(mode.cursor);
    }
    mode.recentre = None;
    *selection = None;
    terminal.set_selection(None)?;
    mode.selection = None;
    mode.selection_mode = CopySelectionMode::Char;
    mode.selecting = false;
    let limit = copy_cursor_limit(&mode.revision, mode.cursor.y, mode_keys_vi, mode.rectangle);
    mode.cursor.x = mode.cursor.x.min(limit);
    Ok(())
}

fn copy_mode_facts(
    mode: &CopyModeState,
    search: Option<&SearchState>,
    word_separators: &WordSeparators,
) -> CopyModeFacts {
    let cursor = PointCoordinate {
        x: mode.cursor.x,
        y: mode
            .cursor
            .y
            .min(mode.revision.total_rows().saturating_sub(1)),
    };
    let reader = mode.revision.reader();
    CopyModeFacts {
        view_mode: mode.kind == FrozenModeKind::View,
        cursor_x: u32::from(cursor.x),
        cursor_y: cursor.y.saturating_sub(mode.viewport_offset),
        cursor_line: format_grid_line(&reader, cursor.y),
        cursor_word: format_grid_word(&reader, cursor, word_separators),
        scroll_position: mode
            .revision
            .maximum_offset()
            .saturating_sub(mode.viewport_offset),
        selection: mode.selection.map(|selection| CopyModeSelectionFacts {
            start_x: u32::from(selection.anchor.x),
            start_y: selection.anchor.y,
            end_x: u32::from(selection.focus.x),
            end_y: selection.focus.y,
        }),
        search_present: mode.search_marks,
        search_count: mode.search_count,
        search_timed_out: false,
        search_match: if mode.search_marks {
            copy_mode_search_match(mode, search, cursor)
        } else {
            String::new()
        },
        rectangle_toggle: mode.rectangle,
        selection_active: mode.selection.is_some() && mode.selecting,
        line_numbers: mode.line_numbers,
        refresh_active: mode.refresh,
    }
}

fn copy_mode_search_match(
    mode: &CopyModeState,
    search: Option<&SearchState>,
    cursor: PointCoordinate,
) -> String {
    let contains = |search: &SearchState, point: PointCoordinate| {
        search
            .matches
            .iter()
            .find(|found| found.contains(point, mode.revision.columns))
            .copied()
    };
    let Some(found) = search.and_then(|search| {
        contains(search, cursor).or_else(|| {
            let back = if cursor.x > 0 {
                PointCoordinate {
                    x: cursor.x - 1,
                    y: cursor.y,
                }
            } else {
                PointCoordinate {
                    x: mode.revision.columns.saturating_sub(1),
                    y: cursor.y.checked_sub(1)?,
                }
            };
            contains(search, back)
        })
    }) else {
        return String::new();
    };
    let mut text = String::new();
    let reader = mode.revision.reader();
    for row in found.row..=found.end_row {
        let Some((start, end)) = found.span(row, mode.revision.columns) else {
            continue;
        };
        for column in start..end {
            let point = PointCoordinate { x: column, y: row };
            if matches!(
                reader.cell(point).width(),
                CellWidth::SpacerTail | CellWidth::SpacerHead
            ) {
                continue;
            }
            reader.push_text(point, &mut text);
        }
    }
    text
}

trait FormatGrid {
    fn grid_columns(&self) -> u16;
    fn grid_rows(&self) -> u32;
    fn grid_row_wrapped(&self, row: u32) -> bool;
    fn grid_cell_width(&self, point: PointCoordinate) -> CellWidth;
    fn grid_first_char(&self, point: PointCoordinate) -> Option<char>;
    fn grid_push_text(&self, point: PointCoordinate, output: &mut String);
}

impl FormatGrid for mode_revision::ModeRevisionReader<'_> {
    fn grid_columns(&self) -> u16 {
        self.columns()
    }

    fn grid_rows(&self) -> u32 {
        self.total_rows()
    }

    fn grid_row_wrapped(&self, row: u32) -> bool {
        self.row_meta(row).wrapped()
    }

    fn grid_cell_width(&self, point: PointCoordinate) -> CellWidth {
        self.cell(point).width()
    }

    fn grid_first_char(&self, point: PointCoordinate) -> Option<char> {
        self.first_char(point)
    }

    fn grid_push_text(&self, point: PointCoordinate, output: &mut String) {
        self.push_text(point, output);
    }
}

impl FormatGrid for ModeRevision {
    fn grid_columns(&self) -> u16 {
        self.columns
    }

    fn grid_rows(&self) -> u32 {
        self.total_rows()
    }

    fn grid_row_wrapped(&self, row: u32) -> bool {
        self.row(row).wrapped()
    }

    fn grid_cell_width(&self, point: PointCoordinate) -> CellWidth {
        self.cell(point).width()
    }

    fn grid_first_char(&self, point: PointCoordinate) -> Option<char> {
        self.first_char(point)
    }

    fn grid_push_text(&self, point: PointCoordinate, output: &mut String) {
        self.push_cell_text(self.cell(point), output);
    }
}

fn format_grid_line_length<G: FormatGrid + ?Sized>(grid: &G, row: u32) -> u32 {
    (0..grid.grid_columns())
        .rev()
        .find(|column| {
            let point = PointCoordinate { x: *column, y: row };
            !matches!(grid.grid_first_char(point), None | Some(' '))
        })
        .map_or(0, |column| u32::from(column).saturating_add(1))
}

fn format_grid_line<G: FormatGrid + ?Sized>(grid: &G, row: u32) -> String {
    let mut text = String::new();
    for column in 0..format_grid_line_length(grid, row) {
        let point = PointCoordinate {
            x: u16::try_from(column).unwrap_or(u16::MAX),
            y: row,
        };
        grid.grid_push_text(point, &mut text);
    }
    text
}

fn format_grid_is_word_separator<G: FormatGrid + ?Sized>(
    grid: &G,
    point: PointCoordinate,
    word_separators: &WordSeparators,
) -> bool {
    if matches!(
        grid.grid_cell_width(point),
        CellWidth::SpacerTail | CellWidth::SpacerHead
    ) {
        return false;
    }
    match grid.grid_first_char(point) {
        None => true,
        Some(character) => {
            character == ' ' || character == '\t' || word_separators.contains_separator(character)
        }
    }
}

fn format_grid_word<G: FormatGrid + ?Sized>(
    grid: &G,
    cursor: PointCoordinate,
    word_separators: &WordSeparators,
) -> String {
    let mut x = cursor.x;
    let mut y = cursor.y;
    let mut found = false;
    loop {
        if format_grid_is_word_separator(grid, PointCoordinate { x, y }, word_separators) {
            found = true;
            break;
        }
        if x == 0 {
            if y == 0 {
                break;
            }
            if !grid.grid_row_wrapped(y.saturating_sub(1)) {
                break;
            }
            y -= 1;
            let length = format_grid_line_length(grid, y);
            if length == 0 {
                break;
            }
            x = u16::try_from(length).unwrap_or(u16::MAX);
        }
        x -= 1;
    }
    let mut text = String::new();
    let last_row = grid.grid_rows().saturating_sub(1);
    loop {
        if found {
            let end = format_grid_line_length(grid, y);
            if end == 0 || u32::from(x) == end.saturating_sub(1) {
                if y == last_row || !grid.grid_row_wrapped(y) {
                    break;
                }
                y += 1;
                x = 0;
            } else {
                x += 1;
            }
        }
        found = true;
        let point = PointCoordinate { x, y };
        if matches!(
            grid.grid_cell_width(point),
            CellWidth::SpacerTail | CellWidth::SpacerHead
        ) {
            continue;
        }
        if format_grid_is_word_separator(grid, point, word_separators) {
            break;
        }
        grid.grid_push_text(point, &mut text);
    }
    text
}

struct LiveGrid<'terminal, 'alloc: 'callbacks, 'callbacks> {
    terminal: &'terminal Terminal<'alloc, 'callbacks>,
    columns: u16,
    rows: u32,
}

impl<'terminal, 'alloc: 'callbacks, 'callbacks> LiveGrid<'terminal, 'alloc, 'callbacks> {
    fn new(terminal: &'terminal Terminal<'alloc, 'callbacks>) -> Result<Self, WorkerError> {
        Ok(Self {
            terminal,
            columns: terminal.cols()?.max(1),
            rows: u32::try_from(terminal.total_rows()?)
                .unwrap_or(u32::MAX)
                .max(1),
        })
    }

    fn active_base(&self) -> Result<u32, WorkerError> {
        Ok(self
            .rows
            .saturating_sub(u32::from(self.terminal.rows()?.max(1))))
    }

    fn reference(&self, point: PointCoordinate) -> Option<libghostty_vt::screen::GridRef<'_>> {
        if point.x >= self.columns || point.y >= self.rows {
            return None;
        }
        self.terminal.grid_ref(Point::Screen(point)).ok()
    }
}

impl FormatGrid for LiveGrid<'_, '_, '_> {
    fn grid_columns(&self) -> u16 {
        self.columns
    }

    fn grid_rows(&self) -> u32 {
        self.rows
    }

    fn grid_row_wrapped(&self, row: u32) -> bool {
        self.reference(PointCoordinate { x: 0, y: row })
            .and_then(|reference| reference.row().ok())
            .and_then(|row| row.is_wrapped().ok())
            .unwrap_or(false)
    }

    fn grid_cell_width(&self, point: PointCoordinate) -> CellWidth {
        self.reference(point)
            .and_then(|reference| reference.cell().ok())
            .and_then(|cell| cell.wide().ok())
            .map_or(CellWidth::Narrow, |wide| match wide {
                CellWide::Narrow => CellWidth::Narrow,
                CellWide::Wide => CellWidth::Wide,
                CellWide::SpacerTail => CellWidth::SpacerTail,
                CellWide::SpacerHead => CellWidth::SpacerHead,
            })
    }

    fn grid_first_char(&self, point: PointCoordinate) -> Option<char> {
        let codepoint = self.reference(point)?.cell().ok()?.codepoint().ok()?;
        char::from_u32(codepoint).filter(|character| *character != '\0')
    }

    fn grid_push_text(&self, point: PointCoordinate, output: &mut String) {
        if matches!(
            self.grid_cell_width(point),
            CellWidth::SpacerTail | CellWidth::SpacerHead
        ) {
            return;
        }
        let Some(reference) = self.reference(point) else {
            return;
        };
        let mut cluster = ['\0'; GRAPHEME_CLUSTER_SCRATCH];
        match reference.graphemes(&mut cluster) {
            Ok(length) => output.extend(cluster.iter().take(length)),
            Err(libghostty_vt::Error::OutOfSpace { required })
                if required > 0 && required <= MAX_GRAPHEME_CLUSTER_CODEPOINTS =>
            {
                let mut cluster = vec!['\0'; required];
                if let Ok(length) = reference.graphemes(&mut cluster) {
                    output.extend(cluster.iter().take(length));
                }
            }
            Err(_) => {}
        }
    }
}

fn pointer_context(
    terminal: &Terminal<'_, '_>,
    mode: Option<&CopyModeState>,
    column: u16,
    row: u16,
    word_separators: &WordSeparators,
) -> Result<PointerContext, WorkerError> {
    let grid = LiveGrid::new(terminal)?;
    let row = u32::from(row);
    let point = PointCoordinate {
        x: column,
        y: grid.active_base()?.saturating_add(row),
    };
    let (word, line) = match mode {
        Some(mode) => {
            let frozen = PointCoordinate {
                x: column,
                y: mode.viewport_offset.saturating_add(row),
            };
            (
                format_grid_word(mode.revision.as_ref(), frozen, word_separators),
                format_grid_line(mode.revision.as_ref(), frozen.y),
            )
        }
        None => (
            format_grid_word(&grid, point, word_separators),
            format_grid_line(&grid, point.y),
        ),
    };
    Ok(PointerContext {
        word,
        line,
        hyperlink: pointer_hyperlink(&grid, point, &mut Vec::new())?,
    })
}

fn pointer_hyperlink(
    grid: &LiveGrid<'_, '_, '_>,
    mut point: PointCoordinate,
    scratch: &mut Vec<u8>,
) -> Result<String, WorkerError> {
    while matches!(
        grid.grid_cell_width(point),
        CellWidth::SpacerTail | CellWidth::SpacerHead
    ) {
        if point.x == 0 {
            return Ok(String::new());
        }
        point.x -= 1;
    }
    let Some(length) = hyperlink_uri_bytes_at(grid.terminal, Point::Screen(point), scratch)? else {
        return Ok(String::new());
    };
    Ok(std::str::from_utf8(&scratch[..length])
        .unwrap_or_default()
        .to_owned())
}

fn snapshot<'alloc: 'callbacks, 'callbacks>(
    terminal: &Terminal<'alloc, 'callbacks>,
    render_state: &mut RenderState<'alloc>,
    rows: &mut RowIterator<'alloc>,
    cells: &mut CellIterator<'alloc>,
    generations: &mut ViewportGenerations,
    change: SnapshotChange,
    dictionary: &mut ViewportDictionary,
    view: Option<&TerminalViewState>,
    status: SessionStatus,
    bound: Option<(u16, u16)>,
) -> Result<TerminalViewport, WorkerError> {
    let started = diagnostic_timer();
    let result = build_snapshot(
        terminal,
        render_state,
        rows,
        cells,
        generations,
        change,
        dictionary,
        view,
        status,
        bound,
    );
    match &result {
        Ok(viewport) => log::trace!(
            target: "zz_terminal::diagnostics::snapshot",
            "build change={change:?} elapsed_us={} generation={} view_generation={} dictionary_generation={} columns={} rows={} cells={} cell_bytes={} styles={} graphemes={} grapheme_bytes={} overlays={} cursor={:?} scrollbar={:?} mode={:?} search={:?} unseen_output={} status={:?}",
            diagnostic_elapsed_us(started),
            viewport.generation,
            viewport.view_generation,
            viewport.dictionary_generation,
            viewport.columns,
            viewport.rows,
            viewport.cells.len(),
            std::mem::size_of_val(viewport.cells.as_ref()),
            viewport.dictionary.styles.len(),
            viewport.dictionary.grapheme_offsets.len(),
            viewport.dictionary.grapheme_bytes.len(),
            viewport.overlays.len(),
            viewport.cursor,
            viewport.scrollbar,
            viewport.mode,
            viewport.search,
            viewport.unseen_output,
            viewport.status,
        ),
        Err(error) => log::trace!(
            target: "zz_terminal::diagnostics::snapshot",
            "build change={change:?} elapsed_us={} error={error}",
            diagnostic_elapsed_us(started),
        ),
    }
    result
}

fn bounded_extent((columns, rows): (u16, u16), bound: Option<(u16, u16)>) -> (u16, u16) {
    bound.map_or((columns, rows), |(most_columns, most_rows)| {
        (columns.min(most_columns.max(1)), rows.min(most_rows.max(1)))
    })
}

fn placement_starts_inside(placement: &KittyPlacement, columns: u16, rows: u16) -> bool {
    placement.viewport_col < i32::from(columns) && placement.viewport_row < i32::from(rows)
}

fn reported_working_directory(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    Some(value.to_owned())
}

#[cfg(test)]
#[path = "session/row_copy_tests.rs"]
mod row_copy_tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "session/profile_tests.rs"]
mod profile_tests;

fn build_snapshot<'alloc: 'callbacks, 'callbacks>(
    terminal: &Terminal<'alloc, 'callbacks>,
    render_state: &mut RenderState<'alloc>,
    rows: &mut RowIterator<'alloc>,
    cells: &mut CellIterator<'alloc>,
    generations: &mut ViewportGenerations,
    change: SnapshotChange,
    dictionary: &mut ViewportDictionary,
    view: Option<&TerminalViewState>,
    status: SessionStatus,
    bound: Option<(u16, u16)>,
) -> Result<TerminalViewport, WorkerError> {
    if let Some(view) = view
        && let Some(copy_mode) = view.copy_mode.as_ref()
    {
        return Ok(copy_mode_snapshot(
            generations,
            change,
            dictionary,
            view,
            copy_mode,
            status,
            bound,
        ));
    }
    let copy_mode = view.and_then(|view| view.copy_mode.as_ref());
    let search = view.and_then(|view| view.search.as_ref());
    let hover_link = view.and_then(|view| view.hover_link.as_ref());
    let unseen_output = view.map_or(0, |view| view.unseen_output);
    render_state.set_clip(
        bound.map_or_else(Clip::default, |(most_columns, most_rows)| Clip {
            y: 0,
            rows: most_rows.max(1),
            cols: most_columns.max(1),
        }),
    )?;
    let snapshot = render_state.update(terminal)?;
    let dirty = snapshot.dirty()?;
    let full_dirty = dirty == Dirty::Full;
    let colors = snapshot.colors()?;
    let default_palette = terminal.default_color_palette()?.0;
    let hints = dictionary.class_hints.clone();
    let classes = Classifier::new(
        &hints,
        &colors.palette,
        &default_palette,
        [colors.foreground, colors.background],
    );
    let (columns, row_count) = bounded_extent((snapshot.cols()?, snapshot.rows()?), bound);
    let foreground = color(colors.foreground);
    let background = color(colors.background);
    let cursor = snapshot
        .cursor_viewport()?
        .filter(|position| position.x < columns && position.y < row_count)
        .map(|position| {
            Cursor::new(
                position.x,
                position.y,
                snapshot.cursor_visible().unwrap_or(true),
                snapshot.cursor_blinking().unwrap_or(false),
                position.at_wide_tail,
                match snapshot
                    .cursor_visual_style()
                    .unwrap_or(CursorVisualStyle::Block)
                {
                    CursorVisualStyle::Bar => CursorStyle::Bar,
                    CursorVisualStyle::Underline => CursorStyle::Underline,
                    CursorVisualStyle::BlockHollow => CursorStyle::BlockHollow,
                    _ => CursorStyle::Block,
                },
                color(colors.cursor.unwrap_or(colors.foreground)),
            )
        })
        .filter(|_| copy_mode.is_none());

    let default_style = classes.default_style(PackedStyle::new(
        foreground,
        background,
        None,
        0,
        UnderlineStyle::None,
    ));
    let cell_count = usize::from(columns).saturating_mul(usize::from(row_count));
    let previous_dictionary_generation = dictionary.generation;
    dictionary.ensure_default(default_style, &colors.palette);
    if dictionary.generation == previous_dictionary_generation && dictionary.should_compact_live() {
        dictionary.reset_live(default_style, &colors.palette);
    }
    let dictionary_reset = dictionary.generation != previous_dictionary_generation;
    let dimensions_changed =
        dictionary.shared_cells.len() != cell_count || dictionary.shared_columns != columns;
    let mut row_copy = std::mem::take(&mut dictionary.row_copy);
    let mut row_styles = std::mem::take(&mut dictionary.row_styles);
    let mut overlays = std::mem::take(&mut dictionary.overlay_scratch);
    let overlay_only = matches!(change, SnapshotChange::Overlay);
    let cell_plane_dirty =
        dimensions_changed || dictionary_reset || (!overlay_only && dirty != Dirty::Clean);
    let preserve_cells = !dimensions_changed && !dictionary_reset && !full_dirty;
    let mut next_cells =
        cell_plane_dirty.then(|| dictionary.acquire_cell_plane(cell_count, preserve_cells));
    let extraction_result = (|| -> Result<(), WorkerError> {
        let mut output_cells = next_cells
            .as_mut()
            .map(|cells| Arc::get_mut(cells).expect("acquired cell plane remains unique"));
        let mut visible_row = 0_u16;
        let mut row_iteration = rows.update(&snapshot)?;
        while visible_row < row_count
            && let Some(row) = row_iteration.next()
        {
            let row_start = usize::from(visible_row).saturating_mul(usize::from(columns));
            if let Some(selected) = row.selection()? {
                overlays.push(OverlaySpan::with_flags(
                    visible_row,
                    selected.start_x.min(columns),
                    selected.end_x.saturating_add(1).min(columns),
                    OverlayKind::Selection,
                    if view
                        .and_then(|view| view.selection.as_ref())
                        .is_some_and(|selection| selection.rectangle)
                    {
                        OVERLAY_RECTANGLE
                    } else {
                        0
                    },
                ));
            }
            let update_cells = output_cells.is_some()
                && (dimensions_changed || dictionary_reset || full_dirty || row.dirty()?);
            if update_cells {
                let row_end = row_start.saturating_add(usize::from(columns));
                let output_row = output_cells
                    .as_deref_mut()
                    .and_then(|cells| cells.get_mut(row_start..row_end))
                    .ok_or(WorkerError::ViewportMetadataTooLarge)?;
                cells
                    .update(row)?
                    .copy_trimmed_into(0, columns, &mut row_copy)?;
                let copied = row_copy.cells().len().min(output_row.len());
                output_row[copied..].fill(PackedCell::EMPTY);
                row_styles.clear();
                row_styles.resize(row_copy.style_count(), None);
                let mut previous = None;
                for (slot, cell) in output_row.iter_mut().zip(row_copy.cells().iter().copied()) {
                    let background = if let Some(index) = cell.bg_color_palette() {
                        CellBackground::Palette(index.0)
                    } else if let Some(value) = cell.bg_color_rgb() {
                        CellBackground::Rgb(value)
                    } else {
                        CellBackground::Style
                    };
                    let hyperlink = cell.has_hyperlink();
                    let style_index = cell.style_index();
                    let plain = background == CellBackground::Style && !hyperlink;
                    let cached = if plain {
                        row_styles.get(style_index).copied().flatten()
                    } else {
                        previous
                            .filter(|(key, _)| *key == (style_index, background, hyperlink))
                            .map(|(_, id)| id)
                    };
                    let style_id = if let Some(id) = cached {
                        id
                    } else {
                        let id = dictionary.intern_style(copied_cell_style(
                            &row_copy.style(style_index)?,
                            background,
                            hyperlink,
                            &colors,
                            &classes,
                        ));
                        if plain {
                            row_styles[style_index] = Some(id);
                        } else {
                            previous = Some(((style_index, background, hyperlink), id));
                        }
                        id
                    };
                    let glyph = match row_copy.grapheme(cell)? {
                        Some(text) => dictionary.encode_glyph(text),
                        None => cell.codepoint().map_or(0, u32::from),
                    };
                    let width = match cell.wide()? {
                        CellWide::Narrow => CellWidth::Narrow,
                        CellWide::Wide => CellWidth::Wide,
                        CellWide::SpacerTail => CellWidth::SpacerTail,
                        CellWide::SpacerHead => CellWidth::SpacerHead,
                    };
                    *slot = PackedCell::new(glyph, style_id, width);
                }
                row.set_dirty(false)?;
            }
            visible_row = visible_row.saturating_add(1);
        }
        Ok(())
    })();
    dictionary.row_copy = row_copy;
    dictionary.row_styles = row_styles;
    if let Err(error) = extraction_result {
        if let Some(cells) = next_cells {
            dictionary.retain_cell_plane(cells);
        }
        overlays.clear();
        dictionary.overlay_scratch = overlays;
        return Err(error);
    }
    if let Some(cells) = next_cells {
        dictionary.commit_cell_plane(cells);
        dictionary.shared_columns = columns;
    }
    snapshot.set_dirty(Dirty::Clean)?;

    if matches!(change, SnapshotChange::Content) {
        generations.content = generations.content.saturating_add(1);
    }
    generations.view = generations.view.saturating_add(1);
    let scrollbar = terminal.scrollbar()?;
    let scrollbar = ScrollbarState {
        total: u32::try_from(scrollbar.total).map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
        offset: u32::try_from(scrollbar.offset)
            .map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
        len: u32::try_from(scrollbar.len).map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
    };
    let kitty_placements = if let Some(kitty) = generations.kitty.as_mut() {
        kitty.placements(terminal, scrollbar)?
    } else {
        Arc::from([])
    };
    let kitty_placements = if kitty_placements
        .iter()
        .all(|placement| placement_starts_inside(placement, columns, row_count))
    {
        kitty_placements
    } else {
        kitty_placements
            .iter()
            .filter(|placement| placement_starts_inside(placement, columns, row_count))
            .cloned()
            .collect()
    };
    if let Some(search) = search {
        for (index, found) in search.matches.iter().enumerate() {
            let start_row = found.row.max(scrollbar.offset);
            let end_row = found.end_row.saturating_add(1).min(
                scrollbar
                    .offset
                    .saturating_add(scrollbar.len.min(u32::from(row_count))),
            );
            for row in start_row..end_row {
                let Some((start, end)) = found.span(row, columns) else {
                    continue;
                };
                overlays.push(OverlaySpan::new(
                    u16::try_from(row.saturating_sub(scrollbar.offset)).unwrap_or(u16::MAX),
                    start,
                    end,
                    if search.current == Some(index) {
                        OverlayKind::SearchCurrent
                    } else {
                        OverlayKind::SearchMatch
                    },
                ));
            }
        }
    }
    if let Some(link) = hover_link
        && link.row < row_count
        && link.start < link.end
    {
        overlays.push(OverlaySpan::new(
            link.row,
            link.start.min(columns),
            link.end.min(columns),
            OverlayKind::LinkHover,
        ));
    }
    let copy_cursor = copy_mode.and_then(|copy_mode| {
        let row = copy_mode.cursor.y.checked_sub(copy_mode.viewport_offset)?;
        (row < u32::from(row_count)).then_some(PointCoordinate {
            x: copy_mode.cursor.x,
            y: row,
        })
    });
    if let Some(point) = copy_cursor
        && point.y < u32::from(row_count)
    {
        overlays.push(OverlaySpan::new(
            u16::try_from(point.y).unwrap_or(u16::MAX),
            point.x.min(columns),
            point.x.saturating_add(1).min(columns),
            OverlayKind::CopyCursor,
        ));
    }
    let mode = if let Some(copy_mode) = copy_mode {
        let total = scrollbar.total.max(1);
        let position = copy_mode.cursor.y.saturating_add(1).min(total);
        match copy_mode.kind {
            FrozenModeKind::Copy => TerminalMode::Copy {
                position,
                total,
                hide_position: copy_mode.hide_position,
            },
            FrozenModeKind::View => TerminalMode::View { position, total },
        }
    } else {
        TerminalMode::Live
    };
    if dictionary_reset {
        dictionary.tune_live_compaction_limits(cell_count);
    }
    let shared_dictionary = dictionary.shared_dictionary();
    let title = terminal.title().unwrap_or("zz");
    let working_directory = terminal.pwd().ok().and_then(reported_working_directory);
    let presentation = dictionary.shared_presentation(
        title,
        working_directory.as_deref(),
        hover_link.map(|link| link.uri.as_str()),
    );
    let overlays = dictionary.finish_overlays(overlays);
    Ok(TerminalViewport {
        generation: generations.content,
        view_generation: generations.view,
        dictionary_generation: dictionary.generation,
        columns,
        rows: row_count,
        foreground,
        background,
        presentation,
        cells: Arc::clone(&dictionary.shared_cells),
        dictionary: shared_dictionary,
        overlays,
        kitty_placements,
        cursor,
        scrollbar,
        mode,
        search: search.map(|search| {
            SearchStatus::new(
                search
                    .current
                    .and_then(|current| u32::try_from(current.saturating_add(1)).ok())
                    .unwrap_or(0),
                u32::try_from(search.matches.len()).unwrap_or(u32::MAX),
            )
            .with_pending(search.pending)
            .with_invalid_pattern(search.invalid_pattern)
        }),
        unseen_output,
        kitty_keyboard: terminal
            .kitty_keyboard_flags()
            .is_ok_and(|flags| !flags.is_empty()),
        mouse_tracking: terminal.is_mouse_tracking()?,
        status,
    })
}

fn copy_mode_snapshot(
    generations: &mut ViewportGenerations,
    change: SnapshotChange,
    dictionary: &mut ViewportDictionary,
    view: &TerminalViewState,
    mode: &CopyModeState,
    status: SessionStatus,
    bound: Option<(u16, u16)>,
) -> TerminalViewport {
    if matches!(change, SnapshotChange::Content) {
        generations.content = generations.content.saturating_add(1);
    }
    generations.view = generations.view.saturating_add(1);
    let revision = &mode.revision;
    let offset = mode.viewport_offset.min(revision.maximum_offset());
    let extent = bounded_extent((revision.columns, revision.viewport_rows), bound);
    let (columns, rows) = extent;
    let cells = dictionary.mode_cells(revision, offset, extent);
    let mut overlays = std::mem::take(&mut dictionary.overlay_scratch);
    if let Some(selection) = mode.selection {
        push_mode_selection_overlays(&mut overlays, extent, selection, offset);
    }
    if let Some(search) = view.search.as_ref().filter(|_| mode.search_marks) {
        let visible_end = offset.saturating_add(u32::from(rows));
        for (index, found) in search.matches.iter().enumerate() {
            let start_row = found.row.max(offset);
            let end_row = found.end_row.saturating_add(1).min(visible_end);
            for row in start_row..end_row {
                let Some((start, end)) = found.span(row, columns) else {
                    continue;
                };
                if start >= end {
                    continue;
                }
                overlays.push(OverlaySpan::new(
                    u16::try_from(row - offset).unwrap_or(u16::MAX),
                    start,
                    end,
                    if search.current == Some(index) {
                        OverlayKind::SearchCurrent
                    } else {
                        OverlayKind::SearchMatch
                    },
                ));
            }
        }
    }
    if let Some(row) = mode.cursor.y.checked_sub(offset)
        && row < u32::from(rows)
        && mode.cursor.x < columns
    {
        overlays.push(OverlaySpan::new(
            u16::try_from(row).unwrap_or(u16::MAX),
            mode.cursor.x,
            mode.cursor.x.saturating_add(1).min(columns),
            OverlayKind::CopyCursor,
        ));
    }
    let total = revision.total_rows().max(1);
    let overlays = dictionary.finish_overlays(overlays);
    let presentation = dictionary.shared_presentation(
        &revision.title,
        revision.working_directory.as_deref(),
        view.hover_link.as_ref().map(|link| link.uri.as_str()),
    );
    TerminalViewport {
        generation: generations.content,
        view_generation: generations.view,
        dictionary_generation: dictionary
            .generation
            .wrapping_add(revision.dictionary_generation()),
        columns,
        rows,
        foreground: revision.foreground,
        background: revision.background,
        presentation,
        cells,
        dictionary: revision.shared_dictionary(),
        overlays,
        kitty_placements: Arc::from([]),
        cursor: None,
        scrollbar: ScrollbarState {
            total,
            offset,
            len: u32::from(revision.viewport_rows),
        },
        mode: match mode.kind {
            FrozenModeKind::Copy => TerminalMode::Copy {
                position: mode.cursor.y.saturating_add(1).min(total),
                total,
                hide_position: mode.hide_position,
            },
            FrozenModeKind::View => TerminalMode::View {
                position: mode.cursor.y.saturating_add(1).min(total),
                total,
            },
        },
        search: view.search.as_ref().map(|search| {
            SearchStatus::new(
                search
                    .current
                    .and_then(|current| u32::try_from(current.saturating_add(1)).ok())
                    .unwrap_or(0),
                u32::try_from(search.matches.len()).unwrap_or(u32::MAX),
            )
            .with_pending(search.pending)
            .with_invalid_pattern(search.invalid_pattern)
        }),
        unseen_output: view.unseen_output,
        kitty_keyboard: false,
        mouse_tracking: false,
        status,
    }
}

fn push_mode_selection_overlays(
    overlays: &mut Vec<OverlaySpan>,
    (columns, rows): (u16, u16),
    selection: ModeSelection,
    offset: u32,
) {
    let (start, end) =
        if (selection.anchor.y, selection.anchor.x) <= (selection.focus.y, selection.focus.x) {
            (selection.anchor, selection.focus)
        } else {
            (selection.focus, selection.anchor)
        };
    let visible_end = offset.saturating_add(u32::from(rows));
    let first_row = start.y.max(offset);
    let last_row = end.y.min(visible_end.saturating_sub(1));
    if first_row > last_row {
        return;
    }
    for row in first_row..=last_row {
        let (span_start, span_end) = if selection.rectangle {
            (
                selection.anchor.x.min(selection.focus.x),
                selection.anchor.x.max(selection.focus.x).saturating_add(1),
            )
        } else if selection.mode == SelectionMode::Line {
            (0, columns)
        } else {
            (
                if row == start.y { start.x } else { 0 },
                if row == end.y {
                    end.x.saturating_add(1)
                } else {
                    columns
                },
            )
        };
        if span_start >= columns || span_start >= span_end {
            continue;
        }
        overlays.push(OverlaySpan::with_flags(
            u16::try_from(row - offset).unwrap_or(u16::MAX),
            span_start,
            span_end.min(columns),
            OverlayKind::Selection,
            if selection.rectangle {
                OVERLAY_RECTANGLE
            } else {
                0
            },
        ));
    }
}

fn style_attributes(
    style: &libghostty_vt::style::Style,
    foreground_explicit_rgb: bool,
    hyperlink: bool,
) -> u16 {
    let mut attributes = 0;
    attributes |= u16::from(style.bold) * ATTR_BOLD;
    attributes |= u16::from(style.italic) * ATTR_ITALIC;
    attributes |= u16::from(style.faint) * ATTR_FAINT;
    attributes |= u16::from(style.blink) * ATTR_BLINK;
    attributes |= u16::from(style.invisible) * ATTR_INVISIBLE;
    attributes |= u16::from(style.strikethrough) * ATTR_STRIKETHROUGH;
    attributes |= u16::from(style.overline) * ATTR_OVERLINE;
    attributes |= u16::from(foreground_explicit_rgb) * ATTR_EXPLICIT_RGB;
    attributes |= u16::from(hyperlink) * ATTR_HYPERLINK;
    attributes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellBackground {
    Style,
    Palette(u8),
    Rgb(RgbColor),
}

fn copied_cell_style(
    style: &libghostty_vt::style::Style,
    background: CellBackground,
    hyperlink: bool,
    colors: &Colors,
    classes: &Classifier<'_>,
) -> PackedStyle {
    let palette = &colors.palette;
    let mut cell_foreground =
        resolve_style_color(style.fg_color, palette).unwrap_or(color(colors.foreground));
    let mut cell_background = match background {
        CellBackground::Style => resolve_style_color(style.bg_color, palette),
        CellBackground::Palette(index) => Some(color(palette[usize::from(index)])),
        CellBackground::Rgb(value) => Some(color(value)),
    }
    .unwrap_or(color(colors.background));
    let (foreground_class, background_class) = if style.inverse {
        std::mem::swap(&mut cell_foreground, &mut cell_background);
        (ColourClass::Resolved, ColourClass::Resolved)
    } else {
        (
            classes.ground(style.fg_color, 0),
            match background {
                CellBackground::Style => classes.ground(style.bg_color, 1),
                CellBackground::Palette(index) => classes.entry(index),
                CellBackground::Rgb(_) => ColourClass::Rgb,
            },
        )
    };
    let foreground_explicit_rgb = matches!(
        if style.inverse {
            style.bg_color
        } else {
            style.fg_color
        },
        StyleColor::Rgb(_)
    );
    PackedStyle::new(
        cell_foreground,
        cell_background,
        resolve_style_color(style.underline_color, palette),
        style_attributes(style, foreground_explicit_rgb, hyperlink),
        underline_style(style.underline),
    )
    .with_classes(foreground_class, background_class)
}

const fn underline_style(underline: Underline) -> UnderlineStyle {
    match underline {
        Underline::None => UnderlineStyle::None,
        Underline::Double => UnderlineStyle::Double,
        Underline::Curly => UnderlineStyle::Curly,
        Underline::Dotted => UnderlineStyle::Dotted,
        Underline::Dashed => UnderlineStyle::Dashed,
        _ => UnderlineStyle::Single,
    }
}

const fn color(value: RgbColor) -> Color {
    Color::rgb(value.r, value.g, value.b)
}

fn resolve_style_color(value: StyleColor, palette: &[RgbColor; 256]) -> Option<Color> {
    match value {
        StyleColor::None => None,
        StyleColor::Palette(index) => palette.get(usize::from(index.0)).copied().map(color),
        StyleColor::Rgb(value) => Some(color(value)),
    }
}

#[derive(Clone, Debug, Default)]
struct ClassHints {
    palette: Vec<(u8, ColourClass)>,
    grounds: [Option<ColourClass>; 2],
    configured: Option<[RgbColor; 2]>,
}

impl ClassHints {
    fn new(appearance: &TerminalAppearance) -> Self {
        Self {
            palette: appearance.palette_classes.clone(),
            grounds: appearance.default_classes,
            configured: Some([
                ghostty_color(appearance.foreground),
                ghostty_color(appearance.background),
            ]),
        }
    }
}

struct Classifier<'a> {
    palette: &'a [RgbColor; 256],
    defaults: &'a [RgbColor; 256],
    entries: &'a [(u8, ColourClass)],
    grounds: [ColourClass; 2],
}

impl<'a> Classifier<'a> {
    fn new(
        hints: &'a ClassHints,
        palette: &'a [RgbColor; 256],
        defaults: &'a [RgbColor; 256],
        current: [RgbColor; 2],
    ) -> Self {
        let grounds = [0, 1].map(|ground| {
            if hints
                .configured
                .as_ref()
                .is_some_and(|configured| configured[ground] != current[ground])
            {
                ColourClass::Rgb
            } else {
                hints.grounds[ground].unwrap_or(ColourClass::Default)
            }
        });
        Self {
            palette,
            defaults,
            entries: &hints.palette,
            grounds,
        }
    }

    fn ground(&self, value: StyleColor, ground: usize) -> ColourClass {
        match value {
            StyleColor::None => self.grounds[ground],
            StyleColor::Palette(index) => self.entry(index.0),
            StyleColor::Rgb(_) => ColourClass::Rgb,
        }
    }

    fn entry(&self, index: u8) -> ColourClass {
        let slot = usize::from(index);
        if self.palette[slot] != self.defaults[slot] {
            return ColourClass::Rgb;
        }
        self.entries
            .iter()
            .find(|(entry, _)| *entry == index)
            .map_or(ColourClass::Palette(index), |(_, class)| *class)
    }

    const fn default_style(&self, style: PackedStyle) -> PackedStyle {
        style.with_classes(self.grounds[0], self.grounds[1])
    }
}

#[cfg(test)]
mod engine_filter_tests;

#[cfg(test)]
mod pointer_context_tests;

#[cfg(test)]
mod search_tests;

#[cfg(test)]
mod viewport_dictionary_tests;

#[cfg(test)]
mod snapshot_tests;

#[cfg(test)]
mod copy_mode_tests;

#[cfg(test)]
mod kitty_graphics_tests;

#[cfg(test)]
mod actor_channels_tests;

#[cfg(test)]
mod pty_io_tests;

#[cfg(all(test, target_os = "linux"))]
mod linux_gather_tests;

#[cfg(test)]
mod input_encoding_tests;

#[cfg(test)]
mod view_actions_tests;

#[cfg(test)]
mod capture_tests;

#[cfg(test)]
mod terminal_responses_tests;

#[cfg(test)]
mod pane_reset_tests;

#[cfg(test)]
mod synchronized_output_tests;

#[cfg(test)]
mod output_view_tests;

#[cfg(test)]
mod spawn_tests;

#[cfg(all(test, unix))]
mod shell_integration_tests;

#[cfg(test)]
mod copy_mode_navigation_tests;

#[cfg(test)]
mod test_support;
