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
mod input_encoding;
use input_encoding::*;
mod search;
use search::*;
mod copy_mode;
use copy_mode::*;
mod capture;
use capture::*;
mod view_actions;
use view_actions::*;
mod publisher;
use publisher::*;
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

#[derive(Debug, Default)]
struct SearchState {
    query: SearchQuery,
    matches: Vec<SearchMatch>,
    current: Option<usize>,
    request_id: u64,
    pending: bool,
    invalid_pattern: bool,
}

#[derive(Default)]
struct ViewportGenerations {
    content: u64,
    view: u64,
    kitty: Option<KittyGraphicsState>,
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

#[derive(Clone)]
struct Publisher {
    event_tx: TerminalEventSender,
    latest: Arc<RwLock<PublishedViewports>>,
    state: Arc<EventQueueState>,
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

#[cfg(test)]
#[path = "session/capture_work_tests.rs"]
mod capture_work_tests;

#[cfg(test)]
#[path = "session/capture_links_tests.rs"]
mod capture_links_tests;

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

#[cfg(test)]
mod search_worker_tests;

struct SearchTarget<'a> {
    view_id: TerminalViewId,
    screen: Screen,
    search: &'a mut SearchSlot,
    snapshot: &'a mut Option<Arc<HistorySearchSnapshot>>,
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
