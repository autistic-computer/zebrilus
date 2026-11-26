use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    ffi::{OsStr, OsString},
    fmt::Write as _,
    fs,
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    sync::{
        Arc, LazyLock, Weak,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use parking_lot::{Condvar, Mutex};
#[cfg(windows)]
use std::io::{Seek, SeekFrom};

mod accounts;
mod agent_account;
mod agent_flip;
#[cfg(feature = "agent")]
mod agent_inbox;
#[cfg(all(feature = "agent", unix))]
mod agent_login;
#[cfg(feature = "agent")]
mod agent_publisher;
#[cfg(feature = "agent")]
mod agent_relay;
mod agent_waits;
mod artifacts;
mod chooser_presentation;
pub(crate) mod cmdq;
#[cfg(unix)]
mod connection;
mod ctrl;
#[cfg(test)]
mod ctrl_tests;
#[cfg(unix)]
mod event_loop;
#[cfg(all(test, unix))]
mod event_loop_tests;
mod exec;
#[cfg(test)]
mod exec_tests;
mod file_commands;
pub(crate) mod file_edit;
#[cfg(unix)]
mod flow;
#[cfg(all(test, unix))]
mod flow_tests;
mod helpers;
mod history_io;
mod hook_commands;
use history_io::*;
#[cfg(unix)]
mod jobs;
#[cfg(unix)]
mod lifecycle;
#[cfg(unix)]
mod loop_handoff;
mod pane_summary;
#[cfg(unix)]
mod pipe_jobs;
#[cfg(test)]
pub(crate) mod solo_tests;
mod source_queue;
pub(crate) mod status_jobs;
mod terminal_reads;
mod terminal_requests;
mod watchers;
pub use exec::exec_resume_kind;
pub(crate) mod path_listing;
mod providers;
mod restart;
pub use restart::{Restart, Stopped};
#[cfg(feature = "agent")]
mod agent_chats;
mod alerts;
mod appearance;
mod buffers;
mod clients;
mod command_output;
mod config_frames;
mod config_loading;
mod control_output;
use control_output::*;
mod effect_clients;
mod gui_requests;
mod kitty_images;
mod pane_modes;
mod pane_pipes;
mod pane_runtime;
mod paste_uploads;
mod prompts;
mod publication;
mod shutdown;
use shutdown::*;
mod connection_messages;
mod simulators;
mod startup;
mod status_refresh;
mod terminal_frames;
mod update_server;
use connection_messages::*;
mod copy_mode;
use copy_mode::*;
mod deferred_terminal_commands;
use deferred_terminal_commands::*;
mod source_paths;
use source_paths::*;
#[cfg(unix)]
mod tmux_shim;
#[cfg(unix)]
use tmux_shim::*;
mod daemon_socket;
use daemon_socket::*;
mod workspace_tools_text;
#[cfg(test)]
use workspace_tools_text::*;
pub use workspace_tools_text::{WORKSPACE_STUB_SKILL, workspace_tools};
mod mode_requests;
use mode_requests::*;
mod terminal_geometry;
use terminal_geometry::*;
mod terminal_views;
use appearance::*;
use terminal_views::*;
mod pane_focus;
use pane_focus::*;
mod client_targets;
use client_targets::*;
mod client_facts;
use client_facts::*;
mod status_requests;
use status_requests::*;
mod format_facts;
use format_facts::*;
mod border_presentations;
use border_presentations::*;
use terminal_frames::*;
mod outbound_mailbox;
use effect_clients::*;
use outbound_mailbox::*;
mod window_extents;
use window_extents::*;
mod mux_effects;
#[cfg(test)]
use config_frames::*;
mod inserted_frames;
use inserted_frames::*;
mod command_routing;
#[cfg(test)]
use command_routing::*;
mod command_requests;
#[cfg(test)]
use status_refresh::*;
mod capture;
#[cfg(test)]
use capture::*;
mod clipboard;
mod command_stdout;
use command_stdout::*;
pub use command_stdout::{CommandStdinSink, append_stdin_payload, command_stdin_sink};
mod inspect;
#[cfg(all(test, feature = "agent"))]
use agent_chats::*;
#[cfg(test)]
use inspect::*;
mod agent_commands;
pub use agent_commands::{agent_send_reads_stdin, send_text_reads_stdin};
#[cfg(feature = "agent")]
mod agent_runtime;
use agent_commands::*;
use clients::*;
use paste_uploads::*;
mod input;
mod key_input;
#[cfg(test)]
use input::*;
use pane_modes::*;
mod overlays;
#[cfg(test)]
use overlays::*;
mod display_panes;
use display_panes::*;
mod choose_buffer;
use choose_buffer::*;
mod choose_tree;
use choose_tree::*;
use config_loading::*;
use publication::*;
mod config;
#[cfg(test)]
use config::*;
use pane_pipes::*;
mod shell_jobs;
pub use buffers::load_buffer_reads_stdin;
use buffers::*;
use shell_jobs::*;
mod wait_channels;
#[cfg(test)]
use gui_requests::*;
use prompts::*;
use zz_daemon_client::{
    DaemonError, DaemonIdentityGuard, default_mux_config, diagnostic_elapsed_us, diagnostic_timer,
    discover_tmux_config, home_directory, mux_config_write_path, terminal_colour_count,
    terminal_feature_mask, terminal_features_list,
    transport::{LocalTransport, Transport, TransportListener, TransportStream},
};
use zz_mux::{
    CUSTOMIZE_MENU_ITEMS, CUSTOMIZE_OUTSIDE_MENU_ITEMS, CellLayout, CommandAliasResolution,
    CommandPromptStep, CommandPromptTemplate, ConfigDiagnostic, CopyModeStyleValues, CustomizeMenu,
    CustomizeMenuItem, CustomizeMode, CustomizeResult, DEFAULT_BUFFER_LIMIT, DetachScope,
    Execution, ExecutionContext, FormatClient, FormatMonitorScope, FormatMonitorTarget,
    FormatNeeds, KeyDecision, KeyEngine, KeyTables, ModeKey, ModePrompt, MouseEventTarget,
    MuxEffect, MuxEngine, PaneKind, PaneModeRequest, PaneRuntimeFacts, PanesMode, ParsedConfig,
    ParsedConfigBytes, PromptOutcome, RetainedJobEnvironment, SourceStream, StatusHooks,
    SwitchAction, TmuxColour, TmuxSort, TmuxSortOrder, WindowSize, canonical_command,
    check_plain_text, command_block_body, copy_mode_action_is_read_only_safe, customize_menu_feed,
    expand_format_bytes, expand_format_values, expand_status, format_command, format_true,
    hook_format_variables, if_shell_truthy, legacy_layouts_in, parse_tmux_colour,
    sanitize_client_output, send_keys_is_read_only_safe, send_keys_target_client, utf8_sanitize,
    validate_static_command_chain,
};
#[cfg(windows)]
use zz_protocol::read_protocol_message_into;
use zz_protocol::{
    BrowserCommand, COMMAND_ARGS_PARSE_BEHAVES, CONFIG_RELOADED_MESSAGE, ChooseBufferAction,
    ChooseBufferItem, ChooseBufferSearchState, ChooseBufferState, ChooseTreeAction, ChooseTreeItem,
    ChooseTreeKind, ChooseTreePaneKind, ChooseTreeSearchState, ChooseTreeState, ChooseTreeTarget,
    ChooserPreviewSize, ChooserRow, ClientEnvironmentBlob, ClientExitAction, ClientFileOperation,
    ClientFileRequest, ClientFileResponse, ClientHello, ClientId, ClientInstanceId, ClientKind,
    ClientMessageKind, ClientPath, ClipboardProducer, CommandInvocation, CommandPromptAction,
    CommandPromptKind, CommandPromptMode, CommandPromptState, CommandPromptType, CommandRequest,
    CommandResolution, CommandResponse, ConfigOverrideEntry, ConfirmAction, ConfirmState,
    ControlSourceFileEvent, DisplayPanesAction, DisplayPanesState, Event, EventPayload,
    GuiResponse, InputMessage, MAX_AGENT_SEND_BYTES, MAX_BROWSER_KEY_REPEAT,
    MAX_CHOOSE_BUFFER_QUERY_BYTES, MAX_CHOOSE_ITEM_KEY_BYTES, MAX_CHOOSE_ITEM_TEXT_BYTES,
    MAX_CHOOSE_TREE_QUERY_BYTES, MAX_ENCODED_FRAME_BYTES, MAX_PANE_INDICATOR_LABEL_BYTES,
    MAX_STARTUP_CONFIG_CAUSE_BYTES, MAX_STARTUP_CONFIG_CAUSES, MAX_STARTUP_CONFIG_CAUSES_BYTES,
    MAX_WINDOW_STATUS_LABEL_BYTES, MENU_ROW_MARGIN, MenuAction, MenuItem, MenuState, MuxOptionKey,
    MuxOptionSource, MuxOptions, MuxSnapshot, PROTOCOL_VERSION, PaneId, PaneIndicator,
    PaneKindSnapshot, PaneMode, PaneStatus, PaneStatusKind, PaneStatusState, PanesModeArea,
    PanesModeBorder, PasteUploadPurpose, PastedImageFormat, PatchTail, PopupBorderLines,
    PreparedCommand, PreparedCommandResult, PromptCursor, ProtocolError, ProtocolMessage, RawText,
    SERVER_OPTION_CAPABILITY_PREFIX, SPLIT_RATIO_BASIS, ServerError, ServerHello, SessionId,
    SessionViewer, SimulatorButton, SimulatorCommand, SimulatorDescriptor, SourceSpan, SplitId,
    StatusLine, StdoutClaim, WindowId, canonical_key, encode_protocol_message_into,
    encode_terminal_patch_event_into, encode_terminal_viewport_event_into, is_key_name,
    layout_menu_row, menu_row_cells, menu_row_width, resolve_command,
};
use zz_terminal::{
    AppearanceColor, AppearanceConfigDisposition, AppearanceLoad, AppearanceProvenance,
    CaptureBoundary, CaptureOptions, ClipboardTarget, Color, ColourClass, CursorBlinkPolicy,
    CursorStyle, DeferredTerminalEvent, EngineKnobs, LastCommandCapture, PasteBufferAction,
    ProgramBlockKind, ProgramState, ProgramStatus, ProgressBarState, TerminalAppearance,
    TerminalCaptureError, TerminalColorScheme, TerminalDiffScratch, TerminalEvent, TerminalEvents,
    TerminalMode, TerminalPalette, TerminalPatchRef, TerminalSession, TerminalSize, TerminalSpawn,
    TerminalViewId, TerminalViewport, ViewStream, WordSeparators, apply_appearance_overrides,
    parse_x11_color, prepare_paste_buffer,
};

#[cfg(feature = "agent")]
use zz_protocol::{
    AgentProvider, AgentSessionOpKind, MAX_AGENT_SESSION_DIRECTORIES, MAX_AGENT_UPDATES_BYTES,
    MAX_GUI_TEXT_BYTES,
};

#[cfg(feature = "agent")]
use crate::agent::{
    environment::AgentWorkspaceEnvironment,
    fanout::{AgentPublisher, AgentRuntime},
    host::{AgentPaneSpec, AgentTurnFailure, HostCommand, PermissionResponse, PromptSource},
    runtime::{AgentSpawnConfig, load_persistent_journal},
    stream::{AgentImage, AgentPrompt, AgentSessionSummary},
};
use crate::{
    configure_shell_job_environment,
    keys::{
        ChooserPromptEdit, choose_buffer_key_action, choose_tree_key_action, chooser_prompt_answer,
        chooser_prompt_edit, chooser_search_edit_key, client_key_inputs, input_key_name,
        overlay_key_action, send_tokens,
    },
    shell_process,
    status::{
        BufferFormatFacts, ClientFormatFacts, ClientViewportFacts, DaemonFormatHooks,
        FormatHookFacts, MessageFormatFacts, StatusRenderer, StatusRequest, client_terminal_facts,
        host_names, live_status_context, status_context, warm_terminfo_entries,
    },
    wake::AcceptWake,
};
#[cfg(not(unix))]
use {std::process::Child, zz_daemon_client::unmasked::SpawnUnmasked as _};

const DIAGNOSTIC_STATE_INTERVAL: Duration = Duration::from_secs(5);
const MAX_CLIENT_KEY_INJECTION_DEPTH: u32 = 16;
const CONTROL_SUBSCRIPTION_INTERVAL: Duration = Duration::from_secs(1);
const CONTROL_CELL_WIDTH_PX: u32 = 8;
const CONTROL_CELL_HEIGHT_PX: u32 = 18;
const WINDOW_SIZE_MAXIMUM: u16 = 10_000;
const MAX_RELIABLE_MESSAGES: usize = 256;
#[cfg(any(test, not(unix)))]
const COPY_PIPE_POLL_INTERVAL: Duration = Duration::from_millis(20);
const COPY_MODE_REFRESH_INTERVAL: Duration = Duration::from_millis(50);
const CONTROL_BUFFER_HIGH: usize = 8192;
const CONTROL_WRITE_MINIMUM: usize = 32;
const CONTROL_PENDING_MESSAGE_LIMIT: usize = MAX_RELIABLE_MESSAGES / 2;
const CONTROL_PENDING_CHUNKS_PER_PANE: usize = 4;
const MAX_COMMAND_PROMPT_OUTPUT_BYTES: usize = 1024 * 1024;

const COMMAND_PROMPT_OUTPUT_TRUNCATED: &str = "… output truncated";
const AGENT_STATE_START_GRACE: Duration = Duration::from_secs(15);
const SEND_TEXT_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
const SEND_TEXT_POLL_INTERVAL: Duration = Duration::from_millis(20);
const RUN_PANE_CAPTURE_LINES: usize = 10_000;
const PASTE_COLLAPSE_MARKERS: [&str; 3] = ["[Pasted text", "[Pasted Content", "[Image"];
const CONTROL_COMMAND_FRAME_FLAGS_NONE: u8 = 0;
const CONTROL_COMMAND_FRAME_FLAGS_CONTROL: u8 = 1;
const SHUTDOWN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_WRITER_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(windows)]
const RESTART_BIND_RETRY: Duration = Duration::from_secs(10);
#[cfg(unix)]
const SIGNAL_SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

fn terminal_status_should_close(status: &zz_terminal::SessionStatus) -> bool {
    matches!(
        status,
        zz_terminal::SessionStatus::Exited(_) | zz_terminal::SessionStatus::Failed(_)
    )
}

fn daemon_color_scheme() -> TerminalColorScheme {
    match std::env::var("ZZ_COLOR_SCHEME").as_deref() {
        Ok("light") => TerminalColorScheme::Light,
        _ => TerminalColorScheme::Dark,
    }
}

#[cfg(unix)]
fn shell_is_valid_for_program(shell: &Path, program: &OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt as _;

    if !shell.is_absolute() {
        return false;
    }
    let Some(shell_name) = shell.file_name() else {
        return false;
    };
    let program = Path::new(program).file_name().unwrap_or(program).as_bytes();
    let program = program.strip_prefix(b"-").unwrap_or(program);
    if shell_name.as_bytes() == program {
        return false;
    }
    rustix::fs::access(shell, rustix::fs::Access::EXEC_OK).is_ok()
}

#[cfg(unix)]
fn shell_is_valid(shell: &Path) -> bool {
    let program = std::env::args_os().next().unwrap_or_default();
    shell_is_valid_for_program(shell, &program)
}

#[cfg(not(unix))]
fn shell_is_valid(shell: &Path) -> bool {
    !shell.as_os_str().is_empty()
}

#[cfg(unix)]
fn terminal_shell_for_session(
    engine: &MuxEngine,
    session: SessionId,
) -> Result<String, ServerError> {
    let shell = engine.default_shell_for_session(session)?;
    if shell_is_valid(Path::new(shell)) {
        Ok(shell.to_owned())
    } else {
        Ok("/bin/sh".to_owned())
    }
}

#[cfg(not(unix))]
fn terminal_shell_for_session(
    engine: &MuxEngine,
    session: SessionId,
) -> Result<String, ServerError> {
    engine.default_shell_for_session(session).map(str::to_owned)
}

fn terminal_environment_for_session(
    engine: &MuxEngine,
    session: SessionId,
) -> Result<Vec<(OsString, Option<OsString>)>, ServerError> {
    let mut environment = engine.environment_for_session(session)?;
    environment.retain(|(name, _)| {
        !name.as_bytes().is_empty()
            && !matches!(
                name.as_str(),
                "TERM" | "TERM_PROGRAM" | "TERM_PROGRAM_VERSION" | "COLORTERM"
            )
    });
    Ok(crate::PARENT_CLAUDE_SESSION_ENVIRONMENT
        .iter()
        .map(|name| (OsString::from(*name), None))
        .chain(
            environment.into_iter().map(|(name, value)| {
                (name.to_os_string(), value.map(|value| value.to_os_string()))
            }),
        )
        .collect())
}

fn tmux_environment(socket_path: &Path, session: Option<SessionId>) -> String {
    let session = session.map_or_else(|| "-1".to_owned(), |session| session.0.to_string());
    format!("{},{},{session}", socket_path.display(), std::process::id())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn process_working_directory(pid: u32) -> Option<PathBuf> {
    zz_daemon_client::process_info::working_directory(pid)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_working_directory(_pid: u32) -> Option<PathBuf> {
    None
}

fn terminal_foreground_process(terminal: &TerminalSession) -> Option<u32> {
    #[cfg(test)]
    edge_facts_tests::count_lookup();
    terminal.foreground_process_id().filter(|pid| *pid != 0)
}

fn terminal_working_directory(terminal: &TerminalSession) -> Option<PathBuf> {
    terminal_foreground_process(terminal).and_then(process_working_directory)
}

fn donor_working_directory(inner: &ServerState, donor: PaneId) -> Option<PathBuf> {
    if let Some(terminal) = inner.terminals.get(&donor) {
        return terminal_working_directory(terminal);
    }
    match &inner.engine.state.pane(donor)?.kind {
        PaneKind::Agent(agent) => agent.cwd.clone().filter(|cwd| cwd.is_dir()),
        _ => None,
    }
}

fn terminal_foreground_facts(terminal: &TerminalSession) -> (Option<u32>, String, Option<String>) {
    let foreground = terminal_foreground_process(terminal);
    let (command, path) = foreground.map_or_else(
        || (String::new(), None),
        |pid| {
            (
                zz_daemon_client::process_info::command_name(pid).unwrap_or_default(),
                process_working_directory(pid).map(|path| path.to_string_lossy().into_owned()),
            )
        },
    );
    (foreground, command, path)
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod process_facts_tests;

fn home_directory_for(engine: &MuxEngine, user: &str) -> Option<String> {
    if user.is_empty() {
        engine
            .global_environment_variable("HOME")
            .filter(|home| !home.is_empty())
            .or_else(|| zz_mux::user_home(None))
    } else {
        zz_mux::user_home(Some(user))
    }
}

pub(crate) fn terminal_current_command(terminal: &TerminalSession) -> String {
    terminal_foreground_process(terminal)
        .and_then(zz_daemon_client::process_info::command_name)
        .unwrap_or_default()
}

#[cfg(test)]
mod edge_facts_tests;

fn host_platform() -> zz_protocol::HostPlatform {
    static PLATFORM: std::sync::OnceLock<zz_protocol::HostPlatform> = std::sync::OnceLock::new();
    *PLATFORM.get_or_init(|| {
        if cfg!(target_os = "macos") {
            zz_protocol::HostPlatform::Macos
        } else if cfg!(target_os = "windows") {
            zz_protocol::HostPlatform::Windows
        } else if cfg!(target_os = "linux") {
            zz_protocol::HostPlatform::Linux(
                ["/etc/os-release", "/usr/lib/os-release"]
                    .iter()
                    .find_map(|path| std::fs::read_to_string(path).ok())
                    .map(|text| zz_protocol::LinuxDistro::from_os_release(&text))
                    .unwrap_or_default(),
            )
        } else {
            zz_protocol::HostPlatform::Unknown
        }
    })
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn command_log_line(command: &CommandInvocation) -> String {
    let mut line = canonical_command(&command.name).to_owned();
    for argument in &command.args {
        line.push(' ');
        line.push_str(argument);
    }
    line
}

fn next_timed_message_id(inner: &mut ServerState) -> u64 {
    inner.history.next_timed_message_id = inner.history.next_timed_message_id.wrapping_add(1);
    inner.history.next_timed_message_id
}

struct OwnedClientMessagePublication {
    client: ClientId,
    pane: Option<PaneId>,
    kind: ClientMessageKind,
    text: String,
    duration_ms: u32,
    message_id: u64,
}

enum DeferredClientEvent {
    Unowned {
        client: ClientId,
        payload: Box<EventPayload>,
    },
    Owned(OwnedClientMessagePublication),
}

fn arm_silence_deadline(
    inner: &mut ServerState,
    window: WindowId,
    seconds: u32,
) -> SilenceDeadline {
    arm_silence_deadline_at(
        inner,
        window,
        Instant::now() + Duration::from_secs(u64::from(seconds)),
    )
}

fn arm_silence_deadline_at(
    inner: &mut ServerState,
    window: WindowId,
    deadline: Instant,
) -> SilenceDeadline {
    inner.panes.next_silence_token = inner.panes.next_silence_token.wrapping_add(1);
    let deadline = SilenceDeadline {
        window,
        token: inner.panes.next_silence_token,
        deadline,
    };
    inner.panes.silence_deadlines.insert(window, deadline);
    deadline
}

fn schedule_window_silence(inner: &mut ServerState, window: WindowId) -> Option<SilenceDeadline> {
    let seconds = inner.engine.monitor_silence_for_window(window);
    if seconds == 0 {
        return None;
    }
    Some(arm_silence_deadline(inner, window, seconds))
}

fn arm_client_message(
    inner: &mut ServerState,
    client: ClientId,
    token: u64,
    duration_ms: u32,
    freeze: bool,
) -> (Option<ActiveClientMessage>, Option<ClientMessageDeadline>) {
    let deadline =
        (duration_ms != 0).then(|| Instant::now() + Duration::from_millis(u64::from(duration_ms)));
    let previous = inner
        .client_entry(client)
        .message
        .replace(ActiveClientMessage {
            token,
            deadline,
            freeze,
        });
    sync_terminal_freeze(inner, client);
    let schedule = deadline.map(|deadline| ClientMessageDeadline {
        client,
        token,
        deadline,
    });
    (previous, schedule)
}

fn take_client_message(inner: &mut ServerState, client: ClientId) -> Option<ActiveClientMessage> {
    let message = inner.client_mut(client).and_then(|c| c.message.take());
    sync_terminal_freeze(inner, client);
    message
}

fn client_terminal_publication_frozen(inner: &ServerState, client: ClientId) -> bool {
    inner
        .client(client)
        .and_then(|c| c.message.as_ref())
        .is_some_and(|message| message.freeze)
        || inner
            .client(client)
            .and_then(|c| c.command_prompt.as_ref())
            .is_some_and(CommandPrompt::freezes)
}

fn push_server_message(inner: &mut ServerState, text: String) -> u64 {
    let number = inner.history.next_message_number;
    inner.history.next_message_number = inner.history.next_message_number.wrapping_add(1);
    inner.history.message_log.push_back(ServerMessage {
        number,
        time: SystemTime::now(),
        text,
    });
    let limit = inner.engine.message_limit();
    while inner.history.message_log.len() > limit {
        inner.history.message_log.pop_front();
    }
    number
}

#[derive(Default)]
struct InertFormatHooks<'a> {
    option_engine: Option<&'a MuxEngine>,
}

impl<'a> InertFormatHooks<'a> {
    fn with_option_engine(engine: &'a MuxEngine) -> Self {
        Self {
            option_engine: Some(engine),
        }
    }
}

impl StatusHooks for InertFormatHooks<'_> {
    fn strftime(&mut self, literal: &str) -> String {
        literal.to_owned()
    }

    fn shell(&mut self, _command: &str, _tag: &zz_mux::FormatJobTag) -> String {
        String::new()
    }

    fn option_variable(&mut self, name: &str, context: &zz_mux::StatusContext) -> Option<RawText> {
        self.option_engine
            .and_then(|engine| engine.format_option_value(context, name))
            .map(RawText::from)
    }
}

fn server_format_context<'e>(
    engine: &'e MuxEngine,
    config_files: &str,
    session: Option<SessionId>,
    window: Option<WindowId>,
    pane: Option<PaneId>,
) -> zz_mux::StatusContext<'e> {
    server_format_context_with_format_client(
        engine,
        config_files,
        session,
        window,
        pane,
        session,
        FormatClient::NoClient,
    )
}

fn server_format_context_with_format_client<'e>(
    engine: &'e MuxEngine,
    config_files: &str,
    session: Option<SessionId>,
    window: Option<WindowId>,
    pane: Option<PaneId>,
    active_session: Option<SessionId>,
    format_client: FormatClient,
) -> zz_mux::StatusContext<'e> {
    let mut context = engine.format_status_context_with_format_client(
        session,
        window,
        pane,
        active_session,
        format_client,
    );
    context.set_format_value("config_files", config_files);
    context
}

fn expanded_style_value(
    engine: &MuxEngine,
    config_files: &str,
    value: &str,
    session: Option<SessionId>,
    window: Option<WindowId>,
    pane: Option<PaneId>,
) -> String {
    if !value.contains("#{") && !value.contains("#(") {
        return value.to_owned();
    }
    let context = server_format_context(engine, config_files, session, window, pane);
    let mut hooks = InertFormatHooks::with_option_engine(engine);
    expand_format_values(value, &context, &mut hooks)
}

#[derive(Clone, Debug)]
pub struct Daemon {
    socket_path: PathBuf,
    load_user_config: bool,
    mux_config_files: Option<Vec<PathBuf>>,
    zz_mux_config_path: Option<PathBuf>,
    server_id: Option<u64>,
    initial_client_working_directory: Option<PathBuf>,
    bootstrap_ready_fd: Option<i32>,
    #[cfg(unix)]
    start_lock_fd: Option<i32>,
    #[cfg(windows)]
    bind_retry: bool,
}

impl Daemon {
    #[must_use]
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            load_user_config: true,
            mux_config_files: None,
            zz_mux_config_path: None,
            server_id: None,
            initial_client_working_directory: None,
            bootstrap_ready_fd: None,
            #[cfg(unix)]
            start_lock_fd: None,
            #[cfg(windows)]
            bind_retry: false,
        }
    }

    #[cfg(unix)]
    #[must_use]
    pub fn with_start_lock_fd(mut self, fd: i32) -> Self {
        self.start_lock_fd = Some(fd);
        self
    }

    #[cfg(windows)]
    #[must_use]
    pub fn with_bind_retry(mut self) -> Self {
        self.bind_retry = true;
        self
    }

    #[must_use]
    pub fn with_bootstrap_ready_fd(mut self, fd: i32) -> Self {
        self.bootstrap_ready_fd = Some(fd);
        self
    }

    #[must_use]
    pub fn with_mux_config_files(mut self, files: impl IntoIterator<Item = PathBuf>) -> Self {
        let files = files.into_iter().collect::<Vec<_>>();
        self.mux_config_files = (!files.is_empty()).then_some(files);
        self
    }

    #[must_use]
    pub fn with_zz_mux_config_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.zz_mux_config_path = Some(path.into());
        self
    }

    #[must_use]
    pub fn with_server_id(mut self, server_id: u64) -> Self {
        self.server_id = Some(server_id);
        self
    }

    #[must_use]
    pub fn with_initial_client_working_directory(
        mut self,
        working_directory: impl Into<PathBuf>,
    ) -> Self {
        self.initial_client_working_directory = Some(working_directory.into());
        self
    }

    #[must_use]
    pub fn without_user_config(mut self) -> Self {
        self.load_user_config = false;
        self
    }

    pub fn run_foreground(&self) -> Result<(), DaemonError> {
        self.run_foreground_with_ready(|_| ())
    }

    pub fn run_foreground_with_ready<R>(
        &self,
        ready: impl FnOnce(u64) -> R,
    ) -> Result<(), DaemonError> {
        self.serve_with_ready(ready).map(drop)
    }

    pub fn serve(&self) -> Result<Stopped, DaemonError> {
        self.serve_with_ready(|_| ())
    }

    fn serve_with_ready<R>(&self, ready: impl FnOnce(u64) -> R) -> Result<Stopped, DaemonError> {
        let mut bootstrap_ready = BootstrapReady::adopt(self.bootstrap_ready_fd);
        #[cfg(unix)]
        let start_lock =
            prepare_socket(&self.socket_path, &mut bootstrap_ready, self.start_lock_fd)?;
        #[cfg(windows)]
        let start_lock = prepare_socket(&self.socket_path, &mut bootstrap_ready)?;
        let listener = self.bind().map_err(|error| {
            if error.kind() == ErrorKind::AddrInUse {
                DaemonError::AlreadyRunning(self.socket_path.clone())
            } else {
                DaemonError::Io(error)
            }
        })?;
        drop(start_lock);
        bootstrap_ready.signal();
        restrict_socket_permissions(&self.socket_path)?;
        listener.set_nonblocking(true)?;
        let socket_guard = SocketGuard::new(self.socket_path.clone());
        let identity_guard = DaemonIdentityGuard::install(&self.socket_path)?;

        self.run_foreground_listener::<LocalTransport, R>(
            listener,
            (socket_guard, identity_guard),
            self.socket_path.display(),
            ready,
        )
    }

    #[cfg(unix)]
    fn bind(&self) -> std::io::Result<<LocalTransport as Transport>::Listener> {
        LocalTransport::bind(&self.socket_path)
    }

    #[cfg(windows)]
    fn bind(&self) -> std::io::Result<<LocalTransport as Transport>::Listener> {
        let deadline = Instant::now() + RESTART_BIND_RETRY;
        loop {
            match LocalTransport::bind(&self.socket_path) {
                Err(error) if self.bind_retry && Instant::now() < deadline => {
                    log::debug!("waiting for the restarted daemon's pipe: {error}");
                    thread::sleep(Duration::from_millis(100));
                }
                result => return result,
            }
        }
    }

    fn run_foreground_listener<T: Transport, R>(
        &self,
        listener: T::Listener,
        socket_guards: (SocketGuard, DaemonIdentityGuard),
        endpoint: impl std::fmt::Display,
        ready: impl FnOnce(u64) -> R,
    ) -> Result<Stopped, DaemonError>
    where
        T::Listener: Send + 'static,
    {
        let (mut socket_guard, identity_guard) = socket_guards;
        let color_scheme = daemon_color_scheme();
        let load = AppearanceLoad::defaults_for(color_scheme);
        log_appearance_load("startup", &load);
        let (appearance, provenance) = (Arc::new(load.appearance), load.provenance);
        let shared = Arc::new(Shared::configured(
            self.server_id.unwrap_or_else(server_id),
            self.server_id.is_some(),
            appearance,
            provenance,
            self.load_user_config,
            paste_upload_directory(&self.socket_path),
            self.socket_path.clone(),
        ));
        shared
            .zz_mux_config_path
            .lock()
            .clone_from(&self.zz_mux_config_path);
        #[cfg(unix)]
        shared.install_tmux_shim()?;
        shared.begin_startup();
        #[cfg(unix)]
        let mut event_loop = event_loop::EventLoop::new::<T>(&listener, &shared)?;
        #[cfg(windows)]
        let accept_shared = Arc::clone(&shared);
        #[cfg(windows)]
        let accept_thread = match thread::Builder::new()
            .name("zz-daemon-accept".to_owned())
            .spawn(move || {
                let result = accept_connections::<T>(&listener, &accept_shared);
                if result.is_err() {
                    accept_shared.request_shutdown();
                }
                (result, listener)
            }) {
            Ok(thread) => thread,
            Err(error) => {
                socket_guard.disarm();
                return Err(DaemonError::Thread(error.to_string()));
            }
        };
        let tailnet = if self.load_user_config && !cfg!(test) {
            let tailnet_shared = Arc::clone(&shared);
            crate::tailnet::spawn(
                self.socket_path.clone(),
                self.socket_path == zz_daemon_client::transport::platform_socket_path(),
                move || tailnet_shared.stopping.load(Ordering::Acquire),
            )
            .inspect_err(|error| log::warn!("could not start the tailnet listener: {error}"))
            .ok()
        } else {
            None
        };
        let complete_startup = || {
            shared.log_diagnostic_snapshot("startup");
            log::info!("zz daemon listening at {endpoint}");
            shared.finish_startup();
            let ready_guard = ready(shared.server_id);
            #[cfg(all(feature = "agent", unix))]
            if let Err(error) = crate::agent::claude_peers::sweep_stale_records() {
                log::warn!(target: "zz::agent", "could not sweep Claude peers: {error}");
            }
            Ok::<R, DaemonError>(ready_guard)
        };
        #[cfg(unix)]
        let (_ready_guard, shutdown_result) = {
            event_loop.start_startup(
                &shared,
                self.load_user_config,
                self.mux_config_files.as_deref(),
                self.initial_client_working_directory.as_deref(),
            )?;
            let mut ready_guard = None;
            let mut startup_result = Ok(());
            let loop_result = event_loop.run::<T>(&listener, &shared, || {
                startup_result = complete_startup().map(|guard| ready_guard = Some(guard));
                if startup_result.is_err() {
                    shared.request_shutdown();
                }
            });
            if loop_result.is_err() {
                shared.request_shutdown();
            }
            (ready_guard, startup_result.and(loop_result))
        };
        #[cfg(windows)]
        let (_ready_guard, shutdown_result, listener) = {
            let startup_result = shared
                .initialize_with_mux_config_files(
                    self.load_user_config,
                    self.mux_config_files.as_deref(),
                    self.initial_client_working_directory.as_deref(),
                )
                .and_then(|()| complete_startup());
            let (ready_guard, startup_result) = match startup_result {
                Ok(guard) => (Some(guard), Ok(())),
                Err(error) => {
                    shared.request_shutdown();
                    (None, Err(error))
                }
            };
            let Ok((accept_result, listener)) = accept_thread.join() else {
                socket_guard.disarm();
                return Err(DaemonError::Thread(
                    "daemon accept thread panicked".to_owned(),
                ));
            };
            if accept_result.is_err() {
                shared.request_shutdown();
            }
            (ready_guard, startup_result.and(accept_result), listener)
        };
        #[cfg(unix)]
        let writers_drained = event_loop.shutdown_completed();
        #[cfg(windows)]
        let writers_drained = false;
        if !writers_drained {
            shared.request_shutdown_after_blockers();
            shared.freeze_response_admissions_and_wait(SHUTDOWN_RESPONSE_TIMEOUT);
            shared.announce_shutdown();
            shared.drain_client_writers_for_shutdown(SHUTDOWN_WRITER_TIMEOUT);
        }
        let restart = shutdown_result.is_ok() && shared.restart_requested.load(Ordering::Acquire);
        if restart && let Some(tailnet) = tailnet {
            shared.stopping.store(true, Ordering::Release);
            let _ = tailnet.join();
        }
        #[cfg(unix)]
        let start_lock = restart
            .then(|| StartLock::acquire(start_lock_path(&self.socket_path)))
            .and_then(|lock| {
                lock.inspect_err(|error| {
                    log::warn!("restarting without the start lock: {error}");
                })
                .ok()
            });
        socket_guard.release();
        drop(socket_guard);
        drop(listener);
        drop(identity_guard);
        #[cfg(unix)]
        if restart {
            drop(shared.tmux_shim.lock().take());
        }
        shutdown_result?;
        #[cfg(feature = "agent")]
        shared.shutdown_agents();
        shared.log_diagnostic_snapshot("shutdown");
        log::info!(
            "zz daemon {}",
            if restart { "restarting" } else { "stopped" }
        );
        log::logger().flush();
        Ok(if restart {
            Stopped::Restart(Restart {
                #[cfg(unix)]
                lock: start_lock,
            })
        } else {
            Stopped::Exit
        })
    }
}

struct BootstrapReady(Option<i32>);

impl BootstrapReady {
    #[cfg(unix)]
    #[allow(
        unsafe_code,
        reason = "the spawning client handed this descriptor to the daemon alone"
    )]
    fn adopt(fd: Option<i32>) -> Self {
        let fd = fd.filter(|fd| unsafe { libc::fcntl(*fd, libc::F_SETFD, libc::FD_CLOEXEC) } == 0);
        Self(fd)
    }

    #[cfg(not(unix))]
    fn adopt(fd: Option<i32>) -> Self {
        let _ = fd;
        Self(None)
    }

    #[cfg(unix)]
    #[allow(
        unsafe_code,
        reason = "one byte to a pipe the daemon owns, then close it"
    )]
    fn signal(&mut self) {
        if let Some(fd) = self.0.take() {
            unsafe {
                let _ = libc::write(fd, [1_u8].as_ptr().cast(), 1);
                libc::close(fd);
            }
        }
    }

    #[cfg(not(unix))]
    fn signal(&mut self) {}
}

impl Drop for BootstrapReady {
    #[cfg(unix)]
    #[allow(unsafe_code, reason = "close a descriptor the daemon owns")]
    fn drop(&mut self) {
        if let Some(fd) = self.0.take() {
            unsafe {
                libc::close(fd);
            }
        }
    }

    #[cfg(not(unix))]
    fn drop(&mut self) {}
}

mod attach;

#[cfg(test)]
mod attach_tests;

#[cfg(test)]
mod pane_frame_tests;

#[cfg(test)]
mod encode_once_tests;

mod shard_sink;

#[cfg(test)]
mod shard_sink_tests;

#[cfg(all(test, unix))]
mod control_sink_tests;

#[cfg(all(test, unix))]
mod direct_write_tests;

#[cfg(all(test, unix))]
mod typed_burst_tests;

#[derive(Default)]
struct BackgroundInsertions {
    pending: BTreeMap<u64, Box<dyn FnOnce() + Send>>,
    draining: bool,
}

struct Shared {
    server: Arc<SharedServer>,
    command_item: Option<Mutex<cmdq::CommandItem<CommandItemContext>>>,
    owner: Option<Arc<Shared>>,
    watcher_effects: Option<Mutex<Vec<watchers::Effect>>>,
}

impl std::ops::Deref for Shared {
    type Target = SharedServer;

    fn deref(&self) -> &Self::Target {
        &self.server
    }
}

struct SharedServer {
    inner: Mutex<ServerState>,
    accept_wake: AcceptWake,
    control_wake: Arc<shard_sink::ControlWake>,
    helpers: helpers::Pool,
    #[cfg(any(windows, test))]
    helper_dispatching: AtomicBool,
    terminal_requests: Arc<terminal_requests::Inbox>,
    agent_state_waits: Mutex<BTreeMap<PaneId, Vec<Weak<agent_waits::TerminalWait>>>>,
    client_writers: Mutex<BTreeMap<ClientId, Arc<OutboundMailbox>>>,
    command_queue_cancels: Mutex<BTreeMap<ClientId, Arc<AtomicBool>>>,
    response_admissions: Mutex<ResponseAdmissionState>,
    response_admissions_changed: Condvar,
    default_attach_effects: Mutex<()>,
    pipe_effects: Mutex<()>,
    #[cfg(unix)]
    pipe_jobs: pipe_jobs::Client,
    #[cfg(all(test, unix))]
    shell_loop_start: Mutex<()>,
    prompt_history_effects: Mutex<history_io::Saves>,
    background_insertions: Mutex<BackgroundInsertions>,
    next_background_insertion: AtomicU64,
    #[cfg(feature = "agent")]
    agent: Mutex<Option<Arc<crate::agent::fanout::AgentRuntime>>>,
    #[cfg(feature = "agent")]
    agent_effects: Mutex<()>,
    #[cfg(feature = "agent")]
    chat_deliveries: Mutex<BTreeMap<PaneId, VecDeque<ChatDelivery>>>,
    #[cfg(feature = "agent")]
    agent_catalog_pending: Mutex<BTreeSet<(ClientId, PaneId, AgentProvider)>>,
    providers: Mutex<providers::Providers>,
    #[cfg(all(feature = "agent", unix))]
    agent_accounts: Mutex<accounts::Accounts>,
    #[cfg(all(feature = "agent", unix))]
    agent_logins: Mutex<agent_login::Logins>,
    #[cfg(feature = "agent")]
    agent_stopped: AtomicBool,
    #[cfg(all(feature = "agent", unix))]
    agent_peers: Mutex<BTreeMap<PaneId, crate::agent::claude_peers::PeerInbox>>,
    #[cfg(feature = "agent")]
    agent_tx: agent_publisher::Sender,
    #[cfg(feature = "agent")]
    agent_rx: Mutex<Option<crossbeam_channel::Receiver<agent_publisher::Message>>>,
    kitty_image_frames: Mutex<BTreeMap<KittyImageKey, KittyImageFrames>>,
    terminal_frames: Arc<TerminalFrames>,
    pasted_images: Mutex<BTreeMap<PaneId, PanePastedImages>>,
    status: Mutex<StatusRenderer>,
    stopping: AtomicBool,
    shutdown_pending: AtomicBool,
    shutdown_forced: AtomicBool,
    shutdown_announced: Mutex<bool>,
    shutdown_blockers: Mutex<ShutdownBlockerState>,
    shutdown_drops_event_hooks: AtomicBool,
    #[cfg(unix)]
    shutdown_cleanup_started: AtomicBool,
    #[cfg(unix)]
    shutdown_cleanup_complete: AtomicBool,
    startup_ready: Mutex<bool>,
    startup_changed: Condvar,
    startup_config_causes: Mutex<Option<Vec<String>>>,
    exit_empty_armed: AtomicBool,
    restart_requested: AtomicBool,
    server_id: u64,
    mux_config_selection: Mutex<(bool, Option<Vec<PathBuf>>)>,
    zz_mux_config_path: Mutex<Option<PathBuf>>,
    paste_directory: PathBuf,
    socket_path: PathBuf,
    #[cfg(test)]
    delivered_wrap_search_commands: Mutex<Vec<bool>>,
    #[cfg(test)]
    response_admission_hook: Mutex<Option<ResponseAdmissionHook>>,
    #[cfg(test)]
    destroy_unattached_hook: Mutex<Option<ResponseAdmissionHook>>,
    #[cfg(test)]
    begin_stopping_hook: Mutex<Option<ResponseAdmissionHook>>,
    #[cfg(unix)]
    tmux_shim: Mutex<Option<TmuxShimGuard>>,
    status_job_needs: crate::status::StatusJobNeeds,
    watcher_tx: watchers::Sender,
    watcher_rx: Mutex<Option<crossbeam_channel::Receiver<watchers::Input>>>,
    timer_tx: timers::TimerSender,
    timer_rx: Mutex<Option<crossbeam_channel::Receiver<timers::TimerInput>>>,
    publish_flush: Mutex<timers::PublishFlush>,
    snapshot_order: Mutex<()>,
    #[cfg(all(feature = "agent", unix))]
    peer_probe: AtomicBool,
    pending_execs: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
    #[cfg(unix)]
    pending_wait_queues: Mutex<Vec<wait_queue::InsertedTask>>,
    #[cfg(windows)]
    exec_links: Mutex<BTreeMap<ClientId, Arc<exec::ExecLink>>>,
    prompt_history_source: Mutex<history_io::Load>,
    prompt_history_settled: AtomicBool,
    connection_threads: Arc<exec::ConnectionThreads>,
    #[cfg(unix)]
    lifecycle: lifecycle::Inbox,
    #[cfg(unix)]
    loop_active: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StartupConfigDelivery {
    Ineligible,
    NoPending,
    Delivered,
    InteractiveOutput(u64),
    AdmissionFailed,
}

impl StartupConfigDelivery {
    const fn delivered(self) -> bool {
        matches!(self, Self::Delivered | Self::InteractiveOutput(_))
    }

    const fn command_output(self) -> Option<u64> {
        match self {
            Self::InteractiveOutput(output_id) => Some(output_id),
            Self::Ineligible | Self::NoPending | Self::Delivered | Self::AdmissionFailed => None,
        }
    }
}

mod hook_events;
use hook_events::*;
#[cfg(all(test, unix))]
mod hook_events_tests;
#[cfg(test)]
use hook_events::mux_hook_events;

enum KeyTableDeadlineCommand {
    Schedule(ClientId, Option<Instant>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SilenceDeadline {
    window: WindowId,
    token: u64,
    deadline: Instant,
}

enum SilenceDeadlineCommand {
    Schedule(SilenceDeadline),
    Cancel { window: WindowId, token: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveClientMessage {
    token: u64,
    deadline: Option<Instant>,
    freeze: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ClientMessageDeadline {
    client: ClientId,
    token: u64,
    deadline: Instant,
}

enum ClientMessageDeadlineCommand {
    Schedule(ClientMessageDeadline),
    Cancel { client: ClientId, token: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct KittyImageKey {
    pane: PaneId,
    image_id: u32,
    generation: u64,
}

struct KittyImageFrames {
    terminal: Weak<TerminalSession>,
    frames: Arc<[Vec<u8>]>,
}

fn resolve_and_prepare_command(
    engine: &MuxEngine,
    command: &CommandInvocation,
) -> Result<(CommandInvocation, bool), ServerError> {
    let (mut command, alias_matched) = match engine.resolve_command_alias(command) {
        CommandAliasResolution::Miss => (command.clone(), false),
        CommandAliasResolution::Expanded(command) => (command, true),
        CommandAliasResolution::MatchedUnsupported(_) => {
            return Err(ServerError::CommandParse(format!(
                "unknown command: {}",
                command.name
            )));
        }
    };
    if (0..command.args.len()).any(|index| command.argument_is_command_block(index)) {
        let owner = canonical_command(&command.name).to_owned();
        engine.prepare_expanded_callback_command(&mut command, &owner, alias_matched)?;
    }
    Ok((command, alias_matched))
}

fn prepare_config_command(
    engine: &MuxEngine,
    command: &CommandInvocation,
) -> Result<(CommandInvocation, bool), ServerError> {
    let (command, alias_matched) = resolve_and_prepare_command(engine, command)?;
    validate_static_command_chain(std::slice::from_ref(&command))?;
    Ok((command, alias_matched))
}

#[cfg(test)]
#[path = "daemon/empty_pane_tests.rs"]
mod empty_pane_tests;

#[cfg(all(test, unix))]
#[path = "daemon/plain_key_echoin_tests.rs"]
mod plain_key_echoin_tests;

#[cfg(test)]
#[path = "daemon/source_hook_queue_tests.rs"]
mod source_hook_queue_tests;

#[cfg(all(test, unix))]
mod chooser_keys_tests;

#[derive(Default)]
struct CommandStreams {
    stdin_available: bool,
    stdin: Option<SourceStream>,
    stdin_error: Option<String>,
    stdout_write: (usize, StdoutClaim),
    stdout: String,
    stderr: String,
    control_error: String,
    exit_code: u8,
    stdout_claim: Option<StdoutClaim>,
    client_exit: bool,
}

#[derive(Default)]
struct CapturedControlCommandEvents {
    events: Vec<EventPayload>,
    callback_parse_events: Vec<(usize, Arc<()>)>,
}

impl CapturedControlCommandEvents {
    fn qualify_callback_parse_events(&mut self, diagnostics: &[CallbackParseFailure]) -> usize {
        let mut qualified = 0;
        for diagnostic in diagnostics {
            let Some(event) = diagnostic.control_event.as_ref() else {
                continue;
            };
            let Some((index, _)) = self
                .callback_parse_events
                .iter()
                .find(|(_, candidate)| Arc::ptr_eq(candidate, event))
            else {
                continue;
            };
            let Some(event) = self.events.get_mut(*index) else {
                continue;
            };
            match event {
                EventPayload::ControlCommandOutput { output } => {
                    output.clear();
                    output.push_str(&diagnostic.message);
                    qualified += 1;
                }
                EventPayload::ControlCommandGuard {
                    output,
                    error: true,
                    sticky_failure: false,
                    ..
                }
                | EventPayload::ControlCommandGuardRaw {
                    output,
                    error: true,
                    sticky_failure: false,
                    ..
                } => {
                    *output = diagnostic.message.clone().into();
                    qualified += 1;
                }
                _ => {}
            }
        }
        self.callback_parse_events.clear();
        qualified
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

struct ControlCommandEventCapture {
    shared: Arc<Shared>,
    client: ClientId,
    item_id: cmdq::ItemId,
    active: bool,
}

impl ControlCommandEventCapture {
    fn finish(mut self) -> CapturedControlCommandEvents {
        self.active = false;
        self.shared
            .pop_control_command_event_capture(self.client, self.item_id)
            .expect("control command event capture frame")
    }
}

impl Drop for ControlCommandEventCapture {
    fn drop(&mut self) {
        if self.active {
            let _ = self
                .shared
                .pop_control_command_event_capture(self.client, self.item_id);
        }
    }
}

impl CommandStreams {
    fn is_empty(&self) -> bool {
        self.stdout.is_empty()
            && self.stderr.is_empty()
            && self.control_error.is_empty()
            && self.exit_code == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClientTerminal {
    NoClient,
    Absent,
    Present,
}

#[derive(Clone, Default)]
struct DetachExit {
    exec: Option<String>,
    parent_hangup: bool,
}

fn context_client_terminal(context: &ExecutionContext) -> ClientTerminal {
    if context.has_no_client() {
        ClientTerminal::NoClient
    } else if context.has_client_terminal() {
        ClientTerminal::Present
    } else {
        ClientTerminal::Absent
    }
}

fn set_context_client_terminal(context: &mut ExecutionContext, terminal: ClientTerminal) {
    match terminal {
        ClientTerminal::NoClient => context.set_no_client(),
        ClientTerminal::Absent => context.set_client_terminal(false),
        ClientTerminal::Present => context.set_client_terminal(true),
    }
}

fn client_terminal(inner: &ServerState, client: ClientId, kind: ClientKind) -> ClientTerminal {
    match kind {
        ClientKind::Control => ClientTerminal::Present,
        ClientKind::Interactive if inner.client(client).is_some_and(|c| c.has_terminal) => {
            ClientTerminal::Present
        }
        ClientKind::Interactive | ClientKind::Command => ClientTerminal::Absent,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommittedTextLane {
    Terminal,
    BrowserSurface,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingCommittedText {
    pane: PaneId,
    lane: CommittedTextLane,
    suppressed_character: Option<char>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ClientFlagState {
    read_only: bool,
    ignore_size: bool,
    no_detach_on_destroy: bool,
}

impl ClientFlagState {
    const fn is_empty(self) -> bool {
        !self.read_only && !self.ignore_size && !self.no_detach_on_destroy
    }

    fn reconnect_flags(self) -> String {
        [
            ("ignore-size", self.ignore_size),
            ("no-detach-on-destroy", self.no_detach_on_destroy),
        ]
        .into_iter()
        .filter_map(|(name, enabled)| enabled.then_some(name))
        .collect::<Vec<_>>()
        .join(",")
    }
}

#[derive(Default)]
struct ClientFlags(BTreeMap<ClientId, ClientFlagState>);

impl ClientFlags {
    fn contains(&self, client: ClientId) -> bool {
        self.0.get(&client).is_some_and(|flags| flags.read_only)
    }

    fn insert(&mut self, client: ClientId) -> bool {
        let flags = self.0.entry(client).or_default();
        !std::mem::replace(&mut flags.read_only, true)
    }

    fn remove(&mut self, client: ClientId) -> bool {
        let Some(flags) = self.0.get_mut(&client) else {
            return false;
        };
        let removed = std::mem::take(&mut flags.read_only);
        if flags.is_empty() {
            self.0.remove(&client);
        }
        removed
    }

    fn clear(&mut self, client: ClientId) {
        self.0.remove(&client);
    }

    fn get(&self, client: ClientId) -> ClientFlagState {
        self.0.get(&client).copied().unwrap_or_default()
    }

    fn apply(&mut self, client: ClientId, requested: &str) {
        for raw in requested.split(',') {
            let (clear, flag) = raw
                .strip_prefix('!')
                .map_or((false, raw), |flag| (true, flag));
            let state = self.0.entry(client).or_default();
            match flag {
                "read-only" if !clear => state.read_only = true,
                "ignore-size" => state.ignore_size = !clear,
                "no-detach-on-destroy" => state.no_detach_on_destroy = !clear,
                _ => {}
            }
            if state.is_empty() {
                self.0.remove(&client);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ColdBootstrapLease {
    #[default]
    Disabled,
    AwaitingFirstExternal,
    Owned(ClientId),
    PendingAbort(ClientId),
    Contested,
    Committed,
}

impl ColdBootstrapLease {
    fn register(&mut self, client: ClientId, startup_reentry: bool) {
        if startup_reentry {
            return;
        }
        *self = match *self {
            Self::AwaitingFirstExternal => Self::Owned(client),
            Self::Owned(_) | Self::PendingAbort(_) => Self::Contested,
            state => state,
        };
    }

    fn prepare(&mut self, client: ClientId, failed: bool) {
        if *self == Self::Owned(client) {
            *self = if failed {
                Self::PendingAbort(client)
            } else {
                Self::Committed
            };
        }
    }

    fn command(&mut self, client: ClientId) {
        if matches!(*self, Self::Owned(owner) | Self::PendingAbort(owner) if owner == client) {
            *self = Self::Committed;
        }
    }

    fn unregister(&mut self, client: ClientId) -> bool {
        match *self {
            Self::PendingAbort(owner) if owner == client => {
                *self = Self::Disabled;
                true
            }
            Self::Owned(owner) if owner == client => {
                *self = Self::Committed;
                false
            }
            _ => false,
        }
    }
}

#[derive(Default)]
struct Client {
    name: Option<String>,
    pid: Option<u32>,
    instance_id: Option<ClientInstanceId>,
    kind: Option<ClientKind>,
    has_terminal: bool,
    exits_on_detach: bool,
    detached: bool,
    features: Option<u32>,
    terminal_type: Option<String>,
    nested: bool,
    remote: bool,
    utf8: bool,
    tty: Option<String>,
    size: Option<(u16, u16)>,
    cell_pixels: Option<(u32, u32)>,
    working_directory: Option<PathBuf>,
    environment: Option<Arc<ClientEnvironmentBlob>>,
    origin: Option<PaneId>,
    color_scheme: Option<TerminalColorScheme>,
    created_time: Option<u64>,
    activity: Option<u64>,
    activity_time: Option<u64>,
    focused: Option<bool>,
    suspended: bool,
    last_session: Option<SessionId>,
    command_prompt: Option<CommandPrompt>,
    choose_tree: Option<ChooseTreeSession>,
    choose_buffer: Option<ChooseBufferSession>,
    published_chooser: [Option<EventPayload>; 3],
    chooser_zoom: Option<WindowId>,
    chooser_under: ChooserUnder,
    display_panes: Option<DisplayPanesSession>,
    message: Option<ActiveClientMessage>,
    message_ignore_keys: bool,
    menu: Option<MenuSession>,
    confirm: Option<ConfirmSession>,
    copy_session: Option<CopySession>,
    key_engine: Option<KeyEngine>,
    published_key_table: Option<(Option<String>, bool)>,
    key_table_deadline: Option<Instant>,
    swallowed_keys: Option<BTreeSet<String>>,
    suppressed_text: Option<BTreeMap<char, u32>>,
    pending_committed_text: Option<VecDeque<PendingCommittedText>>,
    native_terminal_search: bool,
    native_chooser: bool,
    path_picker: bool,
    path_list_root: Option<(u64, Option<PathBuf>)>,
    status_rows: Option<(Vec<String>, String)>,
    terminal_input_sequence: Option<u64>,
    focused_window: Option<WindowId>,
    visible_terminals: Option<BTreeSet<PaneId>>,
    streamed_terminals: Option<BTreeMap<PaneId, TerminalStreamKind>>,
    terminal_preview: bool,
    visible_agents: Option<BTreeSet<PaneId>>,
    #[cfg(feature = "agent")]
    watched_agents: BTreeSet<PaneId>,
    #[cfg(feature = "agent")]
    muted_agents: BTreeSet<PaneId>,
    published_mux_options: Option<MuxOptions>,
    published_terminal_negotiation: Option<TerminalNegotiation>,
    command_output: Option<CommandOutputSession>,
    command_streams: Option<CommandStreams>,
    subscriber: Option<Arc<OutboundMailbox>>,
    control_output: Option<ControlClientOutput>,
    published_snapshot: Option<(u64, u64)>,
    ctrl_subscriptions: Option<zz_protocol::Subscriptions>,
    ctrl_tree_version: Option<u64>,
    ctrl_view: Option<zz_protocol::ClientView>,
    ctrl_layout: Option<(u64, u64)>,
    ctrl_attachment: Option<u64>,
    ctrl_initializing: bool,
    clipboard_query: Option<Instant>,
    mouse_drag: Option<zz_mux::MouseDrag>,
}

impl Client {
    fn local_environment(&self) -> Option<&Arc<ClientEnvironmentBlob>> {
        self.environment.as_ref().filter(|_| !self.remote)
    }

    fn seed_context_environment(&self, context: &mut ExecutionContext) {
        if self.remote {
            context.set_client_environment(None);
        } else if let Some(environment) = &self.environment {
            context.set_client_environment(Some(Arc::clone(environment)));
        }
    }
}

#[derive(Default)]
struct ServerState {
    attachment: Attachment,
    clients: BTreeMap<ClientId, Box<Client>>,
    engine: MuxEngine,
    publication: Publication,
    artifact_state: ArtifactState,
    config: Config,
    waits: Waits,
    terminals: Arc<BTreeMap<PaneId, Arc<TerminalSession>>>,
    panes: PaneTables,
    agents: Agents,
    transfers: Transfers,
    modes: Modes,
    history: History,
    buffers: Buffers,
    jobs: Jobs,
}

#[derive(Default)]
struct Agents {
    published_agent_parents: BTreeMap<PaneId, PaneId>,
    #[cfg(all(feature = "agent", unix))]
    claude_peer_states: BTreeMap<PaneId, String>,
    #[cfg(feature = "agent")]
    agent_panes: BTreeSet<PaneId>,
    #[cfg(all(feature = "agent", unix))]
    armed_flips: BTreeSet<PaneId>,
    #[cfg(all(feature = "agent", unix))]
    armed_accounts: BTreeMap<zz_protocol::AgentProvider, String>,
    #[cfg(feature = "agent")]
    cli_exits: BTreeMap<PaneId, u32>,
    #[cfg(feature = "agent")]
    agent_forks: BTreeMap<PaneId, Option<String>>,
}

#[derive(Default)]
struct Attachment {
    attached: BTreeMap<SessionId, BTreeSet<ClientId>>,
    client_flags: ClientFlags,
    cold_bootstrap: ColdBootstrapLease,
    session_last_attached: BTreeMap<SessionId, u64>,
    border_cycle: parking_lot::Mutex<BTreeSet<ClientId>>,
    window_latest_clients: BTreeMap<WindowId, ClientId>,
    activity_sequence: u64,
    control_routes: BTreeMap<ClientId, ControlRoute>,
    pane_focus: BTreeSet<PaneId>,
    next_client_id: u64,
    destroying_unattached: BTreeSet<SessionId>,
}

#[derive(Default)]
struct PaneTables {
    locations: pane_summary::Locations,
    terminal_geometries: BTreeMap<PaneId, BTreeMap<ClientId, TerminalGeometry>>,
    pane_exit_waits: BTreeMap<PaneId, PaneExitWait>,
    reported_pane_places: BTreeMap<PaneId, BTreeMap<ClientId, ReportedPlace>>,
    pane_read_observations: BTreeMap<PaneId, Weak<terminal_reads::Observation>>,
    silence_deadlines: BTreeMap<WindowId, SilenceDeadline>,
    control_activity_pending: BTreeSet<PaneId>,
    program_status_panes: BTreeSet<PaneId>,
    name_checks: BTreeMap<PaneId, timers::NameCheck>,
    terminal_spawns: BTreeMap<PaneId, TerminalSpawn>,
    popup_panes: BTreeSet<PaneId>,
    scheduled_name_check: Option<Instant>,
    scheduled_window_rename: Option<Instant>,
    preview_watched: BTreeSet<PaneId>,
    terminal_input_sequence: u64,
    next_silence_token: u64,
}

#[derive(Default)]
struct Modes {
    pane_modes: BTreeMap<PaneId, Vec<PaneModeRequest>>,
    pane_mode_transitions: Vec<PaneModeTransition>,
    pane_mode_zooms: BTreeSet<PaneId>,
    panes_mode_frames: BTreeMap<PaneId, PanesModeFrames>,
    chooser_kill_panes: Vec<PaneId>,
    copy_kill_panes: Vec<PaneId>,
    pane_mode_kill_panes: Vec<PaneId>,
    editors: BTreeMap<PaneId, EditorSession>,
    pending_copy_kill: Option<PaneId>,
    pending_copy_source: Option<(PaneId, PaneId)>,
    open_pane_prompts: BTreeMap<ClientId, (u64, PaneId, CommandPromptType)>,
    next_display_panes_token: u64,
}

#[derive(Default)]
struct Config {
    config_files: String,
    appearance: Arc<TerminalAppearance>,
    appearance_provenance: AppearanceProvenance,
    startup_source_client_working_directory: Option<PathBuf>,
    active_color_scheme: TerminalColorScheme,
    published_style_appearance: Option<Arc<TerminalAppearance>>,
    appearance_config_overrides: Vec<ConfigOverrideEntry>,
    mux_options: MuxOptions,
    mux_config_overrides: Vec<ConfigOverrideEntry>,
    mux_option_underlay: MuxOptions,
}

#[derive(Default)]
struct Publication {
    last_published_mux_generation: u64,
    key_table_generations: BTreeMap<String, u64>,
    key_tables_generation: u64,
    ctrl_trees: BTreeMap<(u8, Option<SessionId>), MuxSnapshot>,
    border_presentations_cache: Mutex<Option<CachedBorderPresentations>>,
    status_parameters_cache: Mutex<Option<CachedStatusParameters>>,
    status_preparation_cache: Mutex<Option<CachedStatusPreparation>>,
}

#[derive(Default)]
struct Jobs {
    pane_pipes: BTreeMap<PaneId, PanePipe>,
    #[cfg(unix)]
    shell_jobs: BTreeMap<u64, Arc<Mutex<Option<ShellJobProcess>>>>,
    #[cfg(not(unix))]
    shell_jobs: BTreeMap<u64, Arc<Mutex<Option<Child>>>>,
    active_copy_pipes: usize,
    active_shell_jobs: usize,
    next_shell_job_token: u64,
    next_pipe_token: u64,
}

#[derive(Default)]
struct Buffers {
    paste_buffers: Vec<PasteBuffer>,
    automatic_paste_buffer_limit: AutomaticPasteBufferLimit,
    next_buffer_id: u64,
}

#[derive(Default)]
struct Waits {
    event_waiters: Vec<hook_events::EventWaiter>,
    deferred_event_hooks: Vec<PendingHookEvent>,
    wait_channels: BTreeMap<String, WaitChannel>,
    control_command_event_captures:
        HashMap<(ClientId, cmdq::ItemId), Vec<CapturedControlCommandEvents>>,
    next_command_output_id: u64,
}

#[derive(Default)]
struct History {
    command_history: Vec<String>,
    search_history: Vec<String>,
    message_log: VecDeque<ServerMessage>,
    next_message_number: u64,
    next_timed_message_id: u64,
}

#[derive(Default)]
struct ArtifactState {
    artifacts: artifacts::Artifacts,
    artifacts_watching: bool,
    published_simulators: BTreeMap<PaneId, SimulatorDescriptor>,
}

#[derive(Default)]
struct Transfers {
    pending_gui_requests: BTreeMap<u64, PendingGuiRequest>,
    paste_uploads: BTreeMap<(ClientId, u64), PasteUpload>,
    client_file_waiters: BTreeMap<u64, ClientFileWaiter>,
    file_writes: BTreeMap<(ClientId, u64), file_edit::PendingFileWrite>,
    next_client_file_request: u64,
}

struct WaitItem {
    client: ClientId,
    continuation: cmdq::WaitContinuation,
}

#[derive(Clone)]
struct OverlayWait(Arc<OverlayCompletion>);

struct OverlayCompletion {
    client: ClientId,
    continuation: cmdq::WaitContinuation,
    accepted: Arc<AtomicBool>,
    owner: Weak<Shared>,
    completing: Mutex<()>,
}

impl std::fmt::Debug for OverlayWait {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("OverlayWait")
            .field(&self.0.client)
            .finish()
    }
}

impl OverlayWait {
    fn complete(&self, accepted: bool) {
        let _completing = self.0.completing.lock();
        if self.0.continuation.ready() {
            return;
        }
        self.0.accepted.store(accepted, Ordering::Release);
        if self.0.continuation.complete()
            && let Some(owner) = self.0.owner.upgrade()
        {
            owner.accept_wake.wake();
        }
    }
}

impl Drop for OverlayCompletion {
    fn drop(&mut self) {
        if self.continuation.complete()
            && let Some(owner) = self.owner.upgrade()
        {
            owner.accept_wake.wake();
        }
    }
}

struct OverlayCommands {
    commands: Vec<CommandInvocation>,
    accepted: Arc<AtomicBool>,
}

#[cfg(all(test, unix))]
mod overlay_queue_tests;

struct RegisteredWait {
    name: String,
    continuation: cmdq::WaitContinuation,
    #[cfg(unix)]
    shell: Option<ShellWait>,
    overlay: Option<OverlayCommands>,
    leaf: Option<InsertedLeafContinuation>,
    guard: Option<InsertedControlGuard>,
    terminal: Option<Arc<terminal_requests::CommandState>>,
    file: Option<file_commands::Resume>,
}

#[derive(Default)]
struct WaitChannel {
    locked: bool,
    woken: bool,
    waiters: VecDeque<WaitItem>,
    lockers: VecDeque<WaitItem>,
    granted: Option<WaitItem>,
}

struct ControlRoute {
    mailbox: Arc<OutboundMailbox>,
    panes: BTreeSet<PaneId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ControlPaneOutputMode {
    #[default]
    On,
    Off,
    Paused,
}

#[derive(Default)]
struct ControlPaneOutput {
    mode: ControlPaneOutputMode,
}

struct PendingControlOutput {
    bytes: shard_sink::ControlBytes,
    offset: usize,
    enqueued_at: Instant,
    seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlSubscriptionScope {
    Session,
    Pane(PaneId),
    AllPanes,
    Window(WindowId),
    AllWindows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ControlSubscriptionTarget {
    session: SessionId,
    window: Option<WindowId>,
    window_index: Option<u32>,
    pane: Option<PaneId>,
}

struct ControlSubscription {
    scope: ControlSubscriptionScope,
    format: String,
    previous: BTreeMap<ControlSubscriptionTarget, String>,
}

#[derive(Default)]
struct ControlClientOutput {
    panes: BTreeMap<PaneId, ControlPaneOutput>,
    no_output: bool,
    wait_exit: bool,
    new_layouts: bool,
    pause_after_ms: Option<u64>,
    geometry: Option<TerminalGeometry>,
    window_geometries: BTreeMap<WindowId, TerminalGeometry>,
    subscriptions: BTreeMap<String, ControlSubscription>,
}

#[derive(Debug)]
struct PasteUpload {
    pane: PaneId,
    purpose: PasteUploadPurpose,
    extension: String,
    total_bytes: usize,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct PendingGuiRequest {
    client: ClientId,
    reply: cmdq::Reply<Result<String, String>>,
    screenshot: Option<PendingScreenshot>,
}

#[derive(Debug)]
struct PendingScreenshot {
    path: String,
    png: Vec<u8>,
}

impl ServerState {
    fn border_cycling(&self) -> bool {
        let mut cycling = self.attachment.border_cycle.lock();
        cycling.retain(|client| self.clients.contains_key(client));
        !cycling.is_empty()
    }

    #[inline]
    fn client(&self, id: ClientId) -> Option<&Client> {
        self.clients.get(&id).map(Box::as_ref)
    }

    #[inline]
    fn client_mut(&mut self, id: ClientId) -> Option<&mut Client> {
        self.clients.get_mut(&id).map(Box::as_mut)
    }

    #[inline]
    fn client_entry(&mut self, id: ClientId) -> &mut Client {
        self.clients.entry(id).or_default()
    }

    fn terminals_mut(&mut self) -> &mut BTreeMap<PaneId, Arc<TerminalSession>> {
        Arc::make_mut(&mut self.terminals)
    }
}

mod pane_exit;

#[cfg(all(test, unix))]
mod pane_exit_tests;

struct PaneExitWait {
    current: Arc<pane_exit::Completion>,
    command_wait: Option<Arc<pane_exit::Completion>>,
}

impl PaneExitWait {
    fn new(terminal: &Arc<TerminalSession>) -> Self {
        Self {
            current: pane_exit::Completion::new(terminal),
            command_wait: None,
        }
    }
}

#[derive(Debug)]
struct CommandOutputSession {
    output_id: u64,
    pane: PaneId,
    terminal: Arc<TerminalSession>,
    previous_key_table: Option<String>,
    parked: bool,
}

struct ClientFileWaiter {
    client: ClientId,
    path: PathBuf,
    pane: Option<PaneId>,
    complete: file_commands::Completion,
}

#[derive(Clone, Debug, Default)]
struct OverlayStyleOverrides {
    style: Option<String>,
    selected_style: Option<String>,
    border_style: Option<String>,
}

struct MenuSession {
    state: MenuState,
    commands: Vec<Option<String>>,
    target: ExecutionContext,
    styles: OverlayStyleOverrides,
    mode_tree: Option<ModeTreeMenu>,
}

struct ModeTreeMenu {
    pane: PaneId,
    line: usize,
    outside: bool,
}

#[cfg(all(test, unix))]
mod menu_queue_tests;

enum ConfirmExecution {
    Blocking {
        waiter: OverlayWait,
    },
    Deferred {
        commands: Vec<CommandInvocation>,
        context: Box<ExecutionContext>,
    },
    Background {
        commands: Vec<CommandInvocation>,
    },
}

struct ConfirmSession {
    state: ConfirmState,
    execution: ConfirmExecution,
}

type RetiredCommandOutput = (CommandOutputSession, Option<Arc<OutboundMailbox>>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    CommandPrompt,
    ChooseTree,
    ChooseBuffer,
    DisplayPanes,
    Menu,
    Confirm,
}

fn dismiss_overlays(
    inner: &mut ServerState,
    client: ClientId,
    raising: Option<Overlay>,
    events: &mut Vec<EventPayload>,
    retired: &mut Vec<(ClientId, RetiredCommandOutput)>,
    resume_terminals: &mut bool,
) {
    if raising != Some(Overlay::CommandPrompt)
        && let Some(prompt) = inner
            .client_mut(client)
            .and_then(|c| c.command_prompt.take())
    {
        events.push(EventPayload::CommandPrompt { state: None });
        *resume_terminals |= prompt.freezes();
    }
    if matches!(raising, None | Some(Overlay::ChooseBuffer))
        && inner
            .client_mut(client)
            .and_then(|c| c.choose_tree.take())
            .is_some()
    {
        events.push(EventPayload::ChooseTree { state: None });
    }
    if matches!(raising, None | Some(Overlay::ChooseTree))
        && inner
            .client_mut(client)
            .and_then(|c| c.choose_buffer.take())
            .is_some()
    {
        events.push(EventPayload::ChooseBuffer { state: None });
    }
    if raising != Some(Overlay::DisplayPanes) && dismiss_display_panes(inner, client) {
        events.push(EventPayload::DisplayPanes { state: None });
    }
    if raising != Some(Overlay::CommandPrompt)
        && let Some(output) = take_command_output(inner, client)
    {
        retired.push((client, output));
    }
    if raising != Some(Overlay::Menu)
        && inner
            .client_mut(client)
            .and_then(|c| c.menu.take())
            .is_some()
    {
        events.push(EventPayload::Menu { state: None });
    }
    if raising != Some(Overlay::Confirm)
        && let Some(confirm) = inner.client_mut(client).and_then(|c| c.confirm.take())
    {
        if let ConfirmExecution::Blocking { waiter } = confirm.execution {
            waiter.complete(false);
        }
        events.push(EventPayload::Confirm { state: None });
    }
}

fn take_command_output(inner: &mut ServerState, client: ClientId) -> Option<RetiredCommandOutput> {
    let output = inner
        .client_mut(client)
        .and_then(|c| c.command_output.take())?;
    if !output.parked {
        inner
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(output.previous_key_table.clone());
    }
    let subscriber = inner
        .client(client)
        .and_then(|c| c.subscriber.as_ref())
        .cloned();
    Some((output, subscriber))
}

struct EditorSession {
    client: ClientId,
    path: PathBuf,
    target: EditorTarget,
}

enum EditorTarget {
    Buffer {
        name: String,
        data: Arc<[u8]>,
    },
    Option {
        pane: PaneId,
        edit: zz_mux::CustomizeEdit,
    },
}

#[derive(Clone, Debug)]
struct ServerMessage {
    number: u64,
    time: SystemTime,
    text: String,
}

fn prompt_cursors(inner: &ServerState, session: Option<SessionId>) -> [PromptCursor; 2] {
    inner
        .engine
        .prompt_cursor_options(session)
        .map(|(style, colour)| PromptCursor {
            style,
            colour: zz_protocol::parse_tmux_colour(&if colour.contains("#{") {
                chooser_presentation::expand_without_context(inner, &colour)
            } else {
                colour
            })
            .filter(|colour| {
                !matches!(
                    colour,
                    zz_protocol::TmuxColour::Default | zz_protocol::TmuxColour::Terminal
                )
            }),
        })
}

fn prompt_history(inner: &ServerState, prompt_type: CommandPromptType) -> &[String] {
    match prompt_type {
        CommandPromptType::Command => &inner.history.command_history,
        CommandPromptType::Search => &inner.history.search_history,
    }
}

fn command_prompt_state(inner: &ServerState, client: ClientId) -> Option<CommandPromptState> {
    inner
        .client(client)
        .and_then(|c| c.command_prompt.as_ref())
        .map(|prompt| prompt.state(prompt_history(inner, prompt.prompt_type)))
}

struct CommandPromptSubmission {
    inputs: Vec<String>,
    template: Option<CommandPromptTemplate>,
    source: Option<SourceSpan>,
}

fn append_command_prompt_output(output: &mut String, piece: &str) -> bool {
    let content_limit =
        MAX_COMMAND_PROMPT_OUTPUT_BYTES.saturating_sub(COMMAND_PROMPT_OUTPUT_TRUNCATED.len() + 1);
    let separator_bytes = usize::from(!output.is_empty() && !output.ends_with('\n'));
    let available = content_limit.saturating_sub(output.len());
    if separator_bytes.saturating_add(piece.len()) <= available {
        if separator_bytes != 0 {
            output.push('\n');
        }
        output.push_str(piece);
        return false;
    }

    let piece_available = available.saturating_sub(separator_bytes);
    let mut prefix_bytes = piece.len().min(piece_available);
    while !piece.is_char_boundary(prefix_bytes) {
        prefix_bytes -= 1;
    }
    if prefix_bytes != 0 {
        if separator_bytes != 0 {
            output.push('\n');
        }
        output.push_str(&piece[..prefix_bytes]);
    }
    true
}

fn client_context_pane(inner: &ServerState, client: ClientId) -> Option<PaneId> {
    client_attached_session(inner, client)
        .and_then(|session| inner.engine.state.sessions.get(&session))
        .and_then(|session| {
            inner
                .engine
                .state
                .windows
                .get(&client_focused_window(inner, client, session))
        })
        .map(|window| window.active_pane)
        .or_else(|| {
            inner
                .engine
                .state
                .default_context()
                .map(|(_, _, pane)| pane)
        })
}

fn sync_context_with_attachment(
    inner: &ServerState,
    client: ClientId,
    context: &mut ExecutionContext,
) {
    retarget_context_to_attachment(inner, client, context);
}

fn retarget_context_to_attachment(
    inner: &ServerState,
    client: ClientId,
    context: &mut ExecutionContext,
) {
    let Some(session) = client_attached_session(inner, client)
        .and_then(|session| inner.engine.state.sessions.get(&session))
    else {
        return;
    };
    let window = client_focused_window(inner, client, session);
    let Some(pane) = inner
        .engine
        .state
        .windows
        .get(&window)
        .map(|window| window.active_pane)
    else {
        return;
    };
    context.retarget(&ExecutionContext::new(
        Some(session.id),
        Some(window),
        Some(pane),
    ));
}

#[cfg(test)]
mod copy_page_tests;

fn prepare_command_request(
    inner: &mut ServerState,
    client: ClientId,
    command: &CommandInvocation,
    prepared: bool,
) -> Result<(CommandInvocation, bool), ServerError> {
    inner.attachment.cold_bootstrap.command(client);
    let command = if prepared {
        command.clone()
    } else {
        resolve_and_prepare_command(&inner.engine, command)?.0
    };
    if !prepared && canonical_command(&command.name) == "load-buffer" {
        parse_buffer_command_args("load-buffer", &command.args, &['b', 't'], &['w'])?;
    }
    let guarded = read_only_guard_client(inner, client, &command);
    let blocked = guarded.is_some_and(|guarded| inner.attachment.client_flags.contains(guarded))
        && !read_only_client_may_run(inner, client, &command);
    Ok((command, blocked))
}

fn read_only_client_may_run(
    inner: &ServerState,
    client: ClientId,
    command: &CommandInvocation,
) -> bool {
    if MuxEngine::is_command_alias_group(command) {
        return MuxEngine::command_alias_group_commands(command).is_ok_and(|commands| {
            commands.is_some_and(|commands| {
                commands
                    .iter()
                    .all(|command| read_only_client_may_run(inner, client, command))
            })
        });
    }
    command_is_read_only_safe(command) || control_command_is_read_only_safe(inner, client, command)
}

fn control_command_is_read_only_safe(
    inner: &ServerState,
    client: ClientId,
    command: &CommandInvocation,
) -> bool {
    if inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Control) {
        return false;
    }
    let name = canonical_command(&command.name);
    if name == "refresh-client" {
        return parse_buffer_command_args(
            name,
            &command.args,
            &['A', 'B', 'C', 'F', 'f', 'r', 't'],
            &['c', 'D', 'l', 'L', 'R', 'S', 'U'],
        )
        .ok()
        .and_then(|parsed| parsed.value('t').map(str::to_owned))
        .and_then(|target| find_attached_client_with_aliases(inner, &target, true))
        .is_none_or(|target| target == client);
    }
    hook_events::command_is_read_only(name, &command.args)
}

fn prints_stored_bytes(command: &str) -> bool {
    matches!(
        canonical_command(command),
        "capture-pane" | "save-buffer" | "show-buffer"
    )
}

fn detach_is_inert(inner: &ServerState, client: ClientId) -> bool {
    client_attached_session(inner, client).is_none()
        && inner
            .client(client)
            .is_none_or(|c| c.copy_session.is_none())
        && inner
            .client(client)
            .is_none_or(|c| c.focused_window.is_none())
        && inner
            .client(client)
            .is_none_or(|c| c.visible_terminals.is_none())
        && inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Control)
        && !inner
            .attachment
            .window_latest_clients
            .values()
            .any(|latest| *latest == client)
}

#[cfg(test)]
mod format_universe_tests;

#[cfg(test)]
mod format_buffer_tests;

#[cfg(test)]
mod format_preparation_tests;

#[cfg(test)]
mod format_parameters_tests;

#[cfg(test)]
#[path = "daemon/format_border_tests.rs"]
mod format_border_tests;

#[cfg(unix)]
mod hook_queue;
mod timers;

#[cfg(all(test, unix))]
mod publish_tests;

#[cfg(all(test, unix))]
#[path = "daemon/shell_jobs_tests.rs"]
mod shell_jobs_tests;

#[cfg(all(test, unix))]
#[path = "daemon/pipe_jobs_tests.rs"]
mod pipe_jobs_tests;

#[cfg(all(test, unix))]
mod ptyleak_tests;

fn is_agent_chat(inner: &ServerState, pane: PaneId) -> bool {
    matches!(
        inner.engine.state.pane(pane).map(|pane| &pane.kind),
        Some(PaneKind::Agent(_))
    )
}

#[cfg(feature = "agent")]
fn child_auto_approve(engine: &MuxEngine, pane: PaneId) -> Option<zz_protocol::AgentAutoApprove> {
    use zz_protocol::AgentAutoApprove;

    engine.agent_parent(pane)?;
    (engine.agent_options().auto_approve != AgentAutoApprove::Off).then_some(AgentAutoApprove::All)
}

fn agent_chat_refusal(verb: &str, pane: PaneId) -> ServerError {
    ServerError::InvalidCommand(match verb {
        "pipe-pane" | "show-last-output" | "send-last-output" => {
            format!("{pane} is an agent chat; read it with: zz capture-pane -p -t {pane}")
        }
        "wait-pane" => format!(
            "{pane} is an agent chat; wait for its turn with: {}",
            zz_protocol::agent_mention::wait_command(pane)
        ),
        _ => format!("{pane} is an agent chat; send with: zz agent-send -t {pane} TEXT"),
    })
}

#[cfg(test)]
mod test_shell;

#[cfg(test)]
mod pane_tests;

const READ_ONLY_SAFE_COMMANDS: &[&str] = &[
    "agent-catalog",
    "attach-session",
    "copy-mode",
    "detach-client",
    "list-clients",
    "switch-client",
];

fn read_only_guard_client(
    inner: &ServerState,
    client: ClientId,
    command: &CommandInvocation,
) -> Option<ClientId> {
    if canonical_command(&command.name) != "send-keys" {
        return Some(client);
    }
    match send_keys_target_client(&command.args) {
        Ok(Some(target)) => find_attached_client_with_aliases(inner, &target, true),
        _ => Some(client),
    }
}

fn command_is_read_only_safe(command: &CommandInvocation) -> bool {
    if MuxEngine::is_command_alias_group(command) {
        return MuxEngine::command_alias_group_commands(command).is_ok_and(|commands| {
            commands.is_some_and(|commands| commands.iter().all(command_is_read_only_safe))
        });
    }
    let name = canonical_command(&command.name);
    if name == "send-keys" {
        return send_keys_is_read_only_safe(&command.args).unwrap_or(true);
    }
    READ_ONLY_SAFE_COMMANDS.contains(&name)
}

fn committed_character(input: &zz_terminal::KeyInput, text_follows: bool) -> Option<char> {
    if !text_follows
        || !matches!(
            input.action,
            zz_terminal::KeyAction::Press | zz_terminal::KeyAction::Repeat
        )
    {
        return None;
    }
    if let Some(text) = input.text.as_deref() {
        let mut characters = text.chars();
        if let Some(character) = characters.next()
            && characters.next().is_none()
        {
            return Some(character);
        }
    }
    match input.key {
        zz_terminal::KeyCode::Character(character) => Some(character),
        _ => None,
    }
}

fn suppress_committed_character_locked(
    inner: &mut ServerState,
    client: ClientId,
    pane: PaneId,
    lane: CommittedTextLane,
    character: char,
) {
    let tagged = if let Some(pending) = inner
        .client_mut(client)
        .and_then(|c| c.pending_committed_text.as_mut())
        .and_then(VecDeque::back_mut)
        .filter(|pending| {
            pending.pane == pane && pending.lane == lane && pending.suppressed_character.is_none()
        }) {
        pending.suppressed_character = Some(character);
        true
    } else {
        false
    };
    if !tagged {
        return;
    }
    *inner
        .client_entry(client)
        .suppressed_text
        .get_or_insert_default()
        .entry(character)
        .or_default() += 1;
}

#[cfg(test)]
mod format_tree_tests;

#[cfg(test)]
#[path = "daemon/format_refresh_tests.rs"]
mod format_refresh_tests;

#[cfg(unix)]
#[path = "daemon/wait_queue.rs"]
mod wait_queue;

fn remove_wait_channel_if_unused(channels: &mut BTreeMap<String, WaitChannel>, name: &str) {
    if channels
        .get(name)
        .is_some_and(|channel| !channel.locked && channel.waiters.is_empty() && channel.woken)
    {
        channels.remove(name);
    }
}

fn remove_wait_item(
    inner: &mut ServerState,
    name: &str,
    token: cmdq::ContinuationToken,
    cancelled: bool,
) -> Option<cmdq::WaitContinuation> {
    let channel = inner.waits.wait_channels.get_mut(name)?;
    channel
        .waiters
        .retain(|item| item.continuation.token != token);
    channel
        .lockers
        .retain(|item| item.continuation.token != token);
    let mut wake = None;
    if channel
        .granted
        .as_ref()
        .is_some_and(|item| item.continuation.token == token)
    {
        channel.granted = None;
        if cancelled {
            channel.granted = channel.lockers.pop_front();
            channel.locked = channel.granted.is_some();
            wake = channel
                .granted
                .as_ref()
                .map(|item| item.continuation.clone());
        }
    }
    remove_wait_channel_if_unused(&mut inner.waits.wait_channels, name);
    wake
}

#[cfg(test)]
#[path = "daemon/alias_queue_tests.rs"]
mod alias_queue_tests;

impl FrameExecution for Box<CommandQueueExecution> {
    fn owned(execution: CommandQueueExecution) -> Self {
        Box::new(execution)
    }
}

struct CommandExecutionState {
    draining: bool,
    wait_yields: bool,
    detached: bool,
    deferred_shutdown: Cell<DeferredShutdown>,
    deferred_control_exit: Cell<Option<ClientId>>,
    yielded: Cell<CommandQueueYield>,
    yield_boundary: bool,
    shutdown_blocker: RefCell<Option<ShutdownBlocker>>,
    pending_event_hooks: RefCell<Vec<PendingHookEvent>>,
    deferred_shell_jobs: RefCell<Vec<DeferredShellJob>>,
    callback_parse_failures: RefCell<Vec<CallbackParseFailure>>,
    deferred_config_replay_issues: RefCell<Vec<DeferredConfigReplayIssue>>,
    reported_failures: Cell<bool>,
    frame_active: Cell<bool>,
    child: RefCell<Option<Box<InsertedQueueChild>>>,
    suppress_after_hooks: Cell<bool>,
    suppress_output: Cell<bool>,
}

struct CommandQueueExecution {
    item: cmdq::CommandItem<CommandExecutionState>,
}

impl std::ops::Deref for CommandQueueExecution {
    type Target = CommandExecutionState;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

type DeferredShellJob = Box<dyn FnOnce(&Arc<Shared>) + Send>;

type OverlaySourceThen = Box<dyn FnOnce(&Arc<Shared>) + Send>;

impl CommandQueueExecution {
    fn insert_foreground_child(&self, child: InsertedQueueChild) {
        assert!(
            self.child.borrow_mut().replace(Box::new(child)).is_none(),
            "one child per continuation boundary"
        );
    }

    fn is_draining(&self) -> bool {
        self.draining || self.deferred_shutdown.get() == DeferredShutdown::Force
    }

    fn has_yielded(&self) -> bool {
        self.yielded.get() != CommandQueueYield::None
    }

    fn note_reported_failure(&self) {
        self.reported_failures.set(true);
    }

    fn callback_group_action_since(
        &self,
        start: usize,
        routed_name: &str,
        terminal_depth: usize,
    ) -> Option<CallbackGroupAction> {
        let failures = self.callback_parse_failures.borrow();
        select_callback_group_action(&failures[start..], routed_name, terminal_depth)
    }

    fn handle_callback_group_actions_since(&self, start: usize) {
        for failure in &mut self.callback_parse_failures.borrow_mut()[start..] {
            if failure.owner.is_replay_callback() && failure.depth == 1 {
                failure.group_action = CallbackGroupAction::Handled;
            }
        }
    }

    fn nest_callback_parse_failures_since(&self, start: usize) {
        for failure in &mut self.callback_parse_failures.borrow_mut()[start..] {
            if failure.owner.is_replay_callback() {
                failure.depth += 1;
            }
        }
    }

    fn yield_queue(&self) {
        self.yielded.set(CommandQueueYield::Propagate);
    }

    fn yield_locally(&self) {
        if !self.has_yielded() {
            self.yielded.set(CommandQueueYield::Local);
        }
    }

    fn ensure_shutdown_blocker(&self, shared: &Arc<Shared>) -> bool {
        let mut blocker = self.shutdown_blocker.borrow_mut();
        if blocker.is_none() {
            *blocker = ShutdownBlocker::acquire(shared, self.detached);
        }
        blocker.is_some()
    }

    fn fork_shutdown_blocker(&self, detached: bool) -> Option<ShutdownBlocker> {
        self.shutdown_blocker
            .borrow()
            .as_ref()
            .map(|blocker| blocker.fork(detached))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DeferredShutdown {
    None,
    Recheck,
    Force,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandQueueYield {
    None,
    Local,
    Propagate,
}

fn default_stdout_claim(command: &str, output: &RawText) -> StdoutClaim {
    if output.is_empty() {
        return StdoutClaim::None;
    }
    match canonical_command(command) {
        "save-buffer" | "show-buffer" => StdoutClaim::Raw,
        _ => StdoutClaim::Print,
    }
}

fn daemon_error_text(error: &DaemonError) -> String {
    match error {
        DaemonError::CommandFailed { error, .. } => daemon_error_text(error),
        DaemonError::InsertedCommandParse(message) => message.clone(),
        DaemonError::Server(error) => server_error_text(error),
        error => error.to_string(),
    }
}

fn server_error_text(error: &ServerError) -> String {
    error.tmux_message()
}

fn daemon_error_output(error: &DaemonError) -> Option<&RawText> {
    match error {
        DaemonError::CommandExit { output, .. }
        | DaemonError::ReportedCommandExit { output, .. }
        | DaemonError::CommandFailed { output, .. }
            if !output.as_bytes().is_empty() =>
        {
            Some(output)
        }
        _ => None,
    }
}

fn post_admission_callback_parse_depth(error: &DaemonError) -> usize {
    match error {
        DaemonError::CommandFailed { error, .. } => post_admission_callback_parse_depth(error),
        DaemonError::Server(error) => server_post_admission_callback_parse_depth(error),
        _ => 0,
    }
}

fn command_parse_error(error: &DaemonError) -> bool {
    match error {
        DaemonError::CommandFailed { error, .. } => command_parse_error(error),
        DaemonError::Server(ServerError::CommandParse(_) | ServerError::NativeCommandParse(_)) => {
            true
        }
        _ => false,
    }
}

fn server_post_admission_callback_parse_depth(error: &ServerError) -> usize {
    match error {
        ServerError::PostAdmissionCallback(error) if error.is_command_parse() => {
            1 + server_post_admission_callback_parse_depth(error)
        }
        _ => 0,
    }
}

fn uppercase_first(message: &mut String) {
    let Some(first) = message.chars().next() else {
        return;
    };
    let length = first.len_utf8();
    message.replace_range(..length, &first.to_uppercase().collect::<String>());
}

fn require_no_positionals(
    command: &str,
    parsed: &ParsedBufferCommandArgs,
) -> Result<(), ServerError> {
    if parsed.positional.is_empty() {
        Ok(())
    } else {
        Err(ServerError::CommandParse(format!(
            "{command} does not accept positional arguments"
        )))
    }
}

fn client_file_failure(reason: &str, path: &Path) -> ServerError {
    ServerError::InvalidCommand(format!("{reason}: {}", path.display()))
}

pub(crate) use zz_protocol::agent_stream::AgentBlockPolicy;

fn popup_other_overlay_present(inner: &ServerState, client: ClientId) -> bool {
    inner
        .client(client)
        .is_some_and(|c| c.command_prompt.is_some())
        || inner
            .client(client)
            .is_some_and(|c| c.choose_tree.is_some())
        || inner
            .client(client)
            .is_some_and(|c| c.choose_buffer.is_some())
        || inner
            .client(client)
            .is_some_and(|c| c.display_panes.is_some())
        || inner
            .client(client)
            .is_some_and(|c| c.command_output.is_some())
        || inner.client(client).is_some_and(|c| c.menu.is_some())
        || inner.client(client).is_some_and(|c| c.confirm.is_some())
}

fn any_overlay_present(inner: &ServerState, client: ClientId) -> bool {
    popup_other_overlay_present(inner, client)
}

const MENU_BACK_TAB_KEY: &str = "BTab";

fn expand_popup_value(
    inner: &ServerState,
    target: &ExecutionContext,
    active_session: Option<SessionId>,
    command_name: Option<&str>,
    value: &str,
    target_client: Option<ClientId>,
) -> String {
    expand_command_format(
        inner,
        target,
        active_session,
        command_name,
        value,
        target_client,
        true,
    )
}

fn expand_command_format(
    inner: &ServerState,
    target: &ExecutionContext,
    active_session: Option<SessionId>,
    command_name: Option<&str>,
    value: &str,
    target_client: Option<ClientId>,
    time: bool,
) -> String {
    let mut facts = borrowed_format_hook_facts(inner);
    if let Some(client) = target_client {
        facts.seed.client = client_attached_session(inner, client)
            .map(|session| client_format_facts(inner, client, session));
    }
    let mut variables = target.format_variables.clone();
    variables.insert("config_files".to_owned(), inner.config.config_files.clone());
    let mut hooks = DaemonFormatHooks::command_with_variables(&facts, &variables);
    if let Some(command_name) = command_name {
        hooks = hooks.with_command_item(command_name);
    }
    let format_client = target.target_format_client();
    if time {
        inner.engine.expand_pane_format_time(
            value,
            target,
            active_session,
            format_client,
            &mut hooks,
        )
    } else {
        inner
            .engine
            .expand_pane_format(value, target, active_session, format_client, &mut hooks)
    }
}

struct OptionValue {
    value: String,
    consumed: usize,
}

fn option_value(
    command: &str,
    args: &[RawText],
    index: usize,
    names: &[&str],
) -> Result<Option<OptionValue>, ServerError> {
    let argument = &args[index];
    for name in names {
        if argument == name {
            let value = args.get(index.saturating_add(1)).ok_or_else(|| {
                ServerError::CommandParse(format!("{command} {name} requires a value"))
            })?;
            return Ok(Some(OptionValue {
                value: value.to_string(),
                consumed: 2,
            }));
        }
        let attached = name
            .strip_prefix("--")
            .and_then(|_| argument.strip_prefix(&format!("{name}=")))
            .or_else(|| {
                (!name.starts_with("--"))
                    .then(|| argument.strip_prefix(name))
                    .flatten()
            })
            .filter(|value| !value.is_empty());
        if let Some(value) = attached {
            return Ok(Some(OptionValue {
                value: value.to_owned(),
                consumed: 1,
            }));
        }
    }
    Ok(None)
}

pub use zz_daemon_client::agent_context::{
    WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE, workspace_context,
};

#[derive(Default)]
struct CommandItemContext {
    loop_wait: bool,
    pending_wait: Option<Box<RegisteredWait>>,
    raising_overlay: Option<OverlayWait>,
    result: Option<(CommandResponse, bool)>,
    hook_notifications_only: bool,
    key_table_publish_hold: u32,
    input_focus: Vec<hook_events::FocusProbeScope>,
    client_key_injection_depth: u32,
    deferred_control_notifications: Option<Vec<DeferredControlNotification>>,
    park: Option<(ClientId, u64)>,
    #[cfg(unix)]
    exec_writer: Option<Weak<OutboundMailbox>>,
    #[cfg(unix)]
    loop_leaf: bool,
}

struct DeferredControlNotification {
    payload: EventPayload,
    exclude_client: Option<ClientId>,
    attached_only: bool,
}

#[cfg(test)]
mod command_item_tests;

#[cfg(test)]
mod tests;
