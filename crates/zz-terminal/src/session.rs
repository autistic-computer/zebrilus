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
#[cfg(target_os = "linux")]
mod linux_gather;
#[cfg(target_os = "linux")]
use linux_gather::*;
mod pty_io;
use pty_io::*;
mod snapshot;
use snapshot::*;
mod frames;
use frames::*;
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

#[derive(Debug)]
struct PtyEffects {
    bytes: Vec<u8>,
    overflowed: bool,
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
struct PtyWriter {
    direct: Arc<DirectInput>,
}

#[cfg(all(test, unix))]
#[path = "session/direct_input_echoin_tests.rs"]
mod direct_input_echoin_tests;

#[cfg(unix)]
struct UnixChildKiller(Option<u32>);

#[cfg(all(unix, not(target_os = "linux")))]
struct ChildExitWatch {
    pid: rustix::process::Pid,
    kqueue: Option<std::os::fd::OwnedFd>,
    ready: Option<std::io::Result<ExitStatus>>,
    reaped: bool,
    #[cfg(target_os = "macos")]
    retry: Option<Instant>,
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
struct GatherLease {
    slot: Arc<GatherSlot>,
}

#[cfg(target_os = "linux")]
struct DirectReader {
    gather: Option<PtyGather>,
    pane: Option<GatherPane>,
    last: Instant,
    bytes: usize,
}

#[cfg(any(target_os = "linux", not(unix)))]
struct BufferReturn {
    buffers: Sender<Vec<u8>>,
    #[cfg(target_os = "linux")]
    gather: GatherLease,
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

struct EngineOutput<'a> {
    filter: &'a mut EngineFilter,
    knobs: EngineKnobs,
    renames: &'a mut Vec<String>,
    bar: &'a mut Option<ProgressBar>,
    last_command_status: &'a mut Option<CommandStatusUpdate>,
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

#[cfg(test)]
#[path = "session/live_frames_tests.rs"]
mod live_frames_tests;

#[cfg(test)]
#[path = "session/settle_tests.rs"]
mod settle_tests;

#[cfg(test)]
#[path = "session/row_copy_tests.rs"]
mod row_copy_tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "session/profile_tests.rs"]
mod profile_tests;

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
