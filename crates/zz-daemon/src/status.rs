use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, LazyLock, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use std::thread;

use glob::{MatchOptions, Pattern};
use regex::RegexBuilder;
use zz_daemon_client::{home_directory, unmasked::SpawnUnmasked as _};
use zz_mux::{
    FormatClientRow, FormatEnvironRow, FormatJobTag, FormatNeeds, MuxEngine, StatusContext,
    StatusFormats, StatusHooks, StatusRowVariables, TtyTerm, display_width, expand_status,
};
use zz_protocol::{
    ClientEnvironmentBlob, ClientId, MAX_STATUS_ROWS, MAX_STATUS_TEXT_BYTES, MuxSnapshot, PaneId,
    RawText, SessionId, StatusLine, TmuxColour, WindowId,
};
use zz_terminal::{
    CellWidth, CopyModeFacts, PRIVATE_MODE_NUMBERS, ProgramBlockKind, ProgramStatusRecord,
    ProgressBar, TerminalColorScheme, TerminalFacts, TerminalSession, TerminalViewport,
};

use crate::{configure_shell_job_environment, shell_process};

mod facts;
mod formats;
mod hooks;
mod parts;
mod renderer;
mod shell_jobs;
mod terminfo;

pub(crate) use facts::{
    BufferFormatFacts, ClientFormatFacts, ClientViewportFacts, FormatFactSource, FormatHookFacts,
    MessageFormatFacts, client_environment_rows,
};
pub(crate) use formats::{
    MODE_FORMATS, ModeRequest, expand_modes, expand_style, status_line_needs, status_line_templates,
};
pub(crate) use hooks::vis_escape;
pub(crate) use parts::{completed_status_bytes, status_cache_callbacks};
pub(crate) use renderer::{host_names, live_status_context, status_context};
pub(crate) use terminfo::{
    client_terminal_facts, terminfo_is_warm, warm_terminfo_entries, warm_terminfo_entries_using,
};

use facts::{window_cell_pixels, window_scoped};
use formats::{
    mode_line_numbers, mode_position_variables, mode_presentation, resolve_theme_colours,
    theme_formats,
};
use hooks::cycle_clock_ms;
use parts::{
    completed_status_bytes_with_capture_bytes, expand_status_parts, status_callback_values,
    status_parts_enabled,
};
use renderer::clamp_status_text;
use shell_jobs::{run_shell, shell_second, status_working_directory};

#[cfg(test)]
use hooks::{PANE_OUTPUT_FORMATS, buffer_full, buffer_sample, pane_output_variable};
#[cfg(test)]
use renderer::{render, trim_status_left};
#[cfg(test)]
use terminfo::{parse_infocmp_entries, terminfo_entries};

type ShellCacheKey = (ClientId, FormatJobTag, String);

#[derive(Default)]
struct ShellCacheEntry {
    expanded: Option<String>,
    output: Option<String>,
    output_needs: FormatNeeds,
    last: u64,
    job: Option<ShellJob>,
}

struct ShellJob {
    updates: crossbeam_channel::Receiver<crate::daemon::status_jobs::StatusUpdate>,
    client: crate::daemon::status_jobs::StatusClient,
    fd: i32,
    pid: u32,
    serial: u64,
}

fn format_second() -> i64 {
    i64::try_from(shell_second()).unwrap_or(i64::MAX)
}

pub(crate) const LIST_CLIENTS_CONTEXT_FORMATS: [&str; 1] = ["line"];
pub(crate) const COPY_MODE_CONTEXT_FORMATS: [&str; 19] = [
    "copy_cursor_line",
    "copy_cursor_word",
    "copy_cursor_x",
    "copy_cursor_y",
    "copy_line_numbers",
    "rectangle_toggle",
    "refresh_active",
    "scroll_position",
    "search_count",
    "search_count_partial",
    "search_match",
    "search_present",
    "search_timed_out",
    "selection_active",
    "selection_end_x",
    "selection_end_y",
    "selection_present",
    "selection_start_x",
    "selection_start_y",
];
pub(crate) const SHOW_MESSAGES_CONTEXT_FORMATS: [&str; 3] =
    ["message_number", "message_text", "message_time"];

pub(crate) type StatusJobNeeds = Arc<parking_lot::Mutex<BTreeMap<ClientId, FormatNeeds>>>;

#[derive(Default)]
pub(crate) struct StatusRenderer {
    shell_cache: BTreeMap<ShellCacheKey, ShellCacheEntry>,
    published: BTreeMap<ClientId, Arc<StatusLine>>,
    tmux_shim: Option<PathBuf>,
    zz_executable: Option<PathBuf>,
    job_waker: Option<std::task::Waker>,
    job_client: crate::daemon::status_jobs::StatusClient,
    pending_modes: BTreeSet<ClientId>,
    job_needs: StatusJobNeeds,
    uncovered_jobs: BTreeSet<ClientId>,
    completed: Option<CompletedStatus>,
    animating: BTreeSet<ClientId>,
    animation_started: bool,
    #[cfg(test)]
    expansions: usize,
    owned_clients: BTreeSet<ClientId>,
    owned_client_high_watermark: Option<ClientId>,
}

#[derive(Clone)]
pub(crate) struct StatusRequest {
    pub(crate) client: ClientId,
    pub(crate) formats: Arc<StatusFormats>,
    pub(crate) row_formats: Arc<BTreeMap<u32, String>>,
    pub(crate) option_snapshot: Arc<StatusRowVariables>,
    pub(crate) message_line: u8,
    pub(crate) customized: bool,
    pub(crate) title_format: Arc<Option<String>>,
    pub(crate) environment: Arc<Vec<(RawText, Option<RawText>)>>,
    pub(crate) default_terminal: Arc<String>,
    pub(crate) startup: bool,
    pub(crate) context: Arc<StatusContext<'static>>,
    pub(crate) facts: Arc<FormatHookFacts>,
    pub(crate) client_scheme: Option<TerminalColorScheme>,
    pub(crate) message_styles: Arc<(String, String)>,
    pub(crate) modes: Vec<ModeRequest>,
    pub(crate) pane_borders: Arc<Vec<zz_protocol::PaneBorderPresentation>>,
    pub(crate) references: Arc<BTreeSet<String>>,
}

struct CompletedStatus {
    client: ClientId,
    formats: Arc<StatusFormats>,
    row_formats: Arc<BTreeMap<u32, String>>,
    option_snapshot: Arc<StatusRowVariables>,
    references: Arc<BTreeSet<String>>,
    message_line: u8,
    customized: bool,
    title_format: Arc<Option<String>>,
    context: Arc<StatusContext<'static>>,
    client_scheme: Option<TerminalColorScheme>,
    message_styles: Arc<(String, String)>,
    pane_borders: Arc<Vec<zz_protocol::PaneBorderPresentation>>,
    callback_names: Vec<String>,
    callbacks: Vec<Option<String>>,
    now: i64,
    status: Arc<StatusLine>,
    fact_identity: Weak<FormatHookFacts>,
    request_identity: Weak<StatusRequest>,
    option_bytes: usize,
    parts: Option<Arc<StatusParts>>,
    startup: bool,
    environment_identity: Weak<Vec<(RawText, Option<RawText>)>>,
    default_terminal_identity: Weak<String>,
    context_bytes: usize,
    cycle_tick: Option<u64>,
}

const COMPLETED_STATUS_MAX_BYTES: usize = 1024 * 1024;

pub(crate) const STATUS_CYCLE_PERIOD: std::time::Duration = std::time::Duration::from_millis(100);

thread_local! {
    static CYCLE_ANIMATED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn cycle_tick() -> u64 {
    cycle_clock_ms() / 100
}

pub(crate) fn expand_cycle_frames<T>(expand: impl FnOnce() -> T) -> (T, bool) {
    let before = CYCLE_ANIMATED.replace(false);
    let value = expand();
    let animated = CYCLE_ANIMATED.replace(before);
    (value, animated)
}

struct StatusParts {
    left: Vec<StatusPart>,
    right: Vec<StatusPart>,
    rows: BTreeMap<u32, Vec<StatusPart>>,
    theme: Option<OnceLock<zz_protocol::ThemeColours>>,
    message_styles: Option<OnceLock<(String, String)>>,
}

struct StatusPart {
    range: std::ops::Range<usize>,
    value: Option<OnceLock<String>>,
}

pub(crate) struct DaemonFormatHooks<'a> {
    status_client: Option<ClientId>,
    facts: &'a dyn FormatFactSource,
    option_engine: Option<&'a MuxEngine>,
    status_context: Option<&'a StatusContext<'a>>,
    variables: Option<&'a BTreeMap<String, String>>,
    command_item: Option<&'a str>,
    option_snapshot: Option<&'a StatusRowVariables>,
    cache: Option<&'a mut BTreeMap<ShellCacheKey, ShellCacheEntry>>,
    touched: Option<&'a mut BTreeSet<ShellCacheKey>>,
    refresh: bool,
    now: i64,
    environment: Option<&'a [(RawText, Option<RawText>)]>,
    default_terminal: Option<&'a str>,
    startup: bool,
    tmux_shim: Option<&'a std::path::Path>,
    zz_executable: Option<&'a std::path::Path>,
    job_waker: Option<&'a std::task::Waker>,
    job_client: Option<&'a crate::daemon::status_jobs::StatusClient>,
    facts_withheld: bool,
    buffer_override: Option<BufferFormatFacts>,
    pane_in_mode_override: Option<(PaneId, usize)>,
    live_process_facts: bool,
    cycle: bool,
}

#[cfg(test)]
mod format_status_tests;

#[cfg(test)]
mod hooks_tests;

#[cfg(test)]
mod renderer_tests;

#[cfg(test)]
mod shell_jobs_tests;

#[cfg(test)]
mod terminfo_tests;

#[cfg(test)]
mod test_support;
