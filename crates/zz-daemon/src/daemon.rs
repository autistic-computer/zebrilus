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
mod buffers;
mod clients;
mod command_output;
mod config_frames;
mod config_loading;
mod control_output;
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
mod simulators;
mod startup;
mod status_refresh;
mod update_server;
use effect_clients::*;
mod window_extents;
use window_extents::*;
mod mux_effects;
#[cfg(test)]
use config_frames::*;
mod inserted_frames;
#[cfg(test)]
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
mod inspect;
#[cfg(all(test, feature = "agent"))]
use agent_chats::*;
#[cfg(test)]
use inspect::*;
mod agent_commands;
#[cfg(feature = "agent")]
mod agent_runtime;
#[cfg(test)]
use agent_commands::*;
#[cfg(test)]
use clients::*;
#[cfg(test)]
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
#[cfg(test)]
use display_panes::*;
mod choose_buffer;
use choose_buffer::*;
mod choose_tree;
use choose_tree::*;
use config_loading::*;
#[cfg(test)]
use publication::*;
mod config;
#[cfg(test)]
use config::*;
#[cfg(test)]
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

#[cfg(windows)]
const ACCEPT_WAIT_TIMEOUT: Duration = Duration::from_millis(20);
const DIAGNOSTIC_STATE_INTERVAL: Duration = Duration::from_secs(5);
const MAX_CLIENT_KEY_INJECTION_DEPTH: u32 = 16;
const CONTROL_SUBSCRIPTION_INTERVAL: Duration = Duration::from_secs(1);
const CONTROL_CELL_WIDTH_PX: u32 = 8;
const CONTROL_CELL_HEIGHT_PX: u32 = 18;
const WINDOW_SIZE_MAXIMUM: u16 = 10_000;
const MAX_SOURCE_INVOCATIONS: usize = 50;
const MAX_RELIABLE_MESSAGES: usize = 256;
const MAX_PENDING_TERMINALS: usize = 128;
#[cfg(feature = "agent")]
const MAX_PENDING_AGENT_BYTES: usize = MAX_AGENT_UPDATES_BYTES + MAX_GUI_TEXT_BYTES;
#[cfg(feature = "agent")]
const MAX_PENDING_AGENT_REPLAY_BYTES: usize = 5 * MAX_AGENT_UPDATES_BYTES + MAX_GUI_TEXT_BYTES;
const MAX_OUTBOUND_BYTES: usize = 72 * 1024 * 1024;
const MAX_PREVIEW_OUTBOUND_BYTES: usize = MAX_OUTBOUND_BYTES - MAX_ENCODED_FRAME_BYTES;
const MAX_RECYCLED_FRAME_BUFFERS: usize = 8;
const MAX_RECYCLED_FRAME_CAPACITY: usize = 8 * 1024 * 1024;
#[cfg(any(test, not(unix)))]
const COPY_PIPE_POLL_INTERVAL: Duration = Duration::from_millis(20);
const COPY_MODE_REFRESH_INTERVAL: Duration = Duration::from_millis(50);
const CONTROL_BUFFER_HIGH: usize = 8192;
const CONTROL_WRITE_MINIMUM: usize = 32;
const CONTROL_PENDING_MESSAGE_LIMIT: usize = MAX_RELIABLE_MESSAGES / 2;
const CONTROL_MAXIMUM_AGE: Duration = Duration::from_mins(5);
const CONTROL_PENDING_CHUNKS_PER_PANE: usize = 4;
const MAX_CONTEXT_PATH_BYTES: usize = 4 * 1024;
const MAX_COMMAND_PROMPT_OUTPUT_BYTES: usize = 1024 * 1024;

const COMMAND_PROMPT_OUTPUT_TRUNCATED: &str = "… output truncated";
const AGENT_SEND_WAIT_TIMEOUT: Duration = Duration::from_mins(10);
const AGENT_STATE_START_GRACE: Duration = Duration::from_secs(15);
const SEND_TEXT_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
const SEND_TEXT_POLL_INTERVAL: Duration = Duration::from_millis(20);
const RUN_PANE_CAPTURE_LINES: usize = 10_000;
const SEND_TEXT_TAIL_CHARS: usize = 40;
const PASTE_COLLAPSE_MARKERS: [&str; 3] = ["[Pasted text", "[Pasted Content", "[Image"];
const CONTROL_COMMAND_FRAME_FLAGS_NONE: u8 = 0;
const CONTROL_COMMAND_FRAME_FLAGS_CONTROL: u8 = 1;
const MAX_DISPLAY_PANE_INDICATORS: usize = 4_096;
const MAX_PASTED_IMAGES_PER_PANE: usize = 8;
const MAX_PASTED_IMAGE_BYTES_PER_PANE: usize = 24 * 1024 * 1024;
const SHUTDOWN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_WRITER_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(windows)]
const RESTART_BIND_RETRY: Duration = Duration::from_secs(10);
#[cfg(unix)]
const SIGNAL_SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
#[cfg(unix)]
static TMUX_SHIM_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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

fn client_lifecycle_event(
    inner: &ServerState,
    name: &'static str,
    client: ClientId,
) -> Option<PendingHookEvent> {
    let kind = inner.client(client).and_then(|c| c.kind)?;
    if !matches!(kind, ClientKind::Interactive | ClientKind::Control) {
        return None;
    }
    let registered = inner.client(client);
    let origin = registered
        .and_then(|c| c.origin)
        .or_else(|| {
            let environment = registered.and_then(Client::local_environment)?;
            ["TMUX_PANE", "ZZ_PANE"]
                .into_iter()
                .find_map(|name| environment.map().get(name)?.as_str().parse().ok())
        })
        .and_then(|pane| ExecutionContext::for_pane(&inner.engine.state, pane));
    let context = client_attached_session(inner, client).map_or_else(
        || {
            if let Some(origin) = origin {
                return origin;
            }
            inner.engine.state.most_recent_context().map_or_else(
                ExecutionContext::default,
                |(session, window, pane)| {
                    ExecutionContext::new(Some(session), Some(window), Some(pane))
                },
            )
        },
        |session| hook_events::live_session_context(&inner.engine.state, session),
    );
    let client_name = client_format_name(inner, client);
    let mut event = PendingHookEvent::client(name, context, client, Some(client_name.as_str()));
    if let Some(session) = event.context.session {
        event
            .variables
            .insert(HOOK_SESSION_CONTEXT_FORMAT.to_owned(), session.to_string());
        if let Some(state) = inner.engine.state.sessions.get(&session) {
            event.variables.insert(
                HOOK_SESSION_NAME_CONTEXT_FORMAT.to_owned(),
                state.name.clone(),
            );
        }
    }
    if let Some(window) = event.context.window {
        event
            .variables
            .insert(HOOK_WINDOW_CONTEXT_FORMAT.to_owned(), window.to_string());
        if let Some(state) = inner.engine.state.windows.get(&window) {
            event.variables.insert(
                HOOK_WINDOW_NAME_CONTEXT_FORMAT.to_owned(),
                state.name.clone(),
            );
            event
                .variables
                .insert("hook_window_index".to_owned(), state.index.to_string());
        }
    }
    if let Some(pane) = event.context.pane {
        event
            .variables
            .insert(HOOK_PANE_CONTEXT_FORMAT.to_owned(), pane.to_string());
    }
    Some(event)
}

fn pane_event_if_observed(
    inner: &mut ServerState,
    name: &'static str,
    pane: PaneId,
) -> Option<PendingHookEvent> {
    let waited = hook_events::event_waited_in(&inner.waits.event_waiters, name);
    let event = PendingHookEvent::live_pane(name, pane, &inner.engine)?;
    let hooked = inner
        .engine
        .event_hook_commands(&event.context, name)
        .is_some_and(|commands| !commands.is_empty());
    if !hooked && !waited {
        inner
            .engine
            .note_event_hook_fired(&event.context, name, unix_timestamp());
    }
    (hooked || waited).then_some(event)
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

fn resolve_appearance(
    color_scheme: TerminalColorScheme,
    entries: &[ConfigOverrideEntry],
) -> AppearanceLoad {
    let mut load = AppearanceLoad::defaults_for(color_scheme);
    load.root = zz_config::config_path_for_write().ok();
    apply_appearance_overrides(load, entries)
}

fn send_terminal_themes(
    outbound: &Arc<OutboundMailbox>,
    listing: &Arc<Mutex<Option<u64>>>,
    request_id: u64,
) {
    if listing.lock().replace(request_id).is_some() {
        return;
    }
    let outbound = Arc::clone(outbound);
    let pending = Arc::clone(listing);
    let listed = std::thread::Builder::new()
        .name("zz-terminal-themes".to_owned())
        .spawn(move || {
            let themes = zz_terminal::enumerate_ghostty_themes(
                zz_config::config_path_for_write().ok().as_deref(),
            )
            .into_iter()
            .filter(|theme| theme.name.len() <= zz_protocol::MAX_TERMINAL_THEME_NAME_BYTES)
            .take(zz_protocol::MAX_TERMINAL_THEMES)
            .map(|theme| zz_protocol::TerminalTheme::new(theme.name, &theme.appearance))
            .collect();
            let Some(request_id) = pending.lock().take() else {
                return;
            };
            let _ = outbound
                .enqueue_reliable(&ProtocolMessage::TerminalThemesResponse { request_id, themes });
        });
    if let Err(error) = listed {
        listing.lock().take();
        log::warn!("could not list terminal themes: {error}");
    }
}

const CURSOR_STYLE_OVERRIDES: [(&str, Option<(CursorStyle, CursorBlinkPolicy)>); 7] = [
    ("default", None),
    (
        "blinking-block",
        Some((CursorStyle::Block, CursorBlinkPolicy::On)),
    ),
    ("block", Some((CursorStyle::Block, CursorBlinkPolicy::Off))),
    (
        "blinking-underline",
        Some((CursorStyle::Underline, CursorBlinkPolicy::On)),
    ),
    (
        "underline",
        Some((CursorStyle::Underline, CursorBlinkPolicy::Off)),
    ),
    (
        "blinking-bar",
        Some((CursorStyle::Bar, CursorBlinkPolicy::On)),
    ),
    ("bar", Some((CursorStyle::Bar, CursorBlinkPolicy::Off))),
];

struct ResolvedTerminalWorkerOptions {
    appearance: Arc<TerminalAppearance>,
    allow_passthrough: bool,
    knobs: EngineKnobs,
    wrap_search: bool,
}

fn terminal_worker_options(
    engine: &MuxEngine,
    base_appearance: &Arc<TerminalAppearance>,
    config_files: &str,
    pane: PaneId,
) -> Result<ResolvedTerminalWorkerOptions, ServerError> {
    let options = engine.terminal_worker_options_for_pane(pane)?;
    let window_colours = window_style_colours(engine, config_files, &options, pane);
    Ok(ResolvedTerminalWorkerOptions {
        appearance: pane_terminal_appearance(
            base_appearance,
            options.cursor_style,
            &options.cursor_colour,
            window_colours,
            &options.pane_colours,
        ),
        allow_passthrough: options.allow_passthrough,
        knobs: EngineKnobs {
            scroll_on_clear: options.scroll_on_clear,
            alternate_screen: options.alternate_screen,
            allow_rename: options.allow_rename,
            erase_byte: options.erase_byte,
            verase_byte: options.verase_byte,
            mode_keys_vi: options.mode_keys_vi,
        },
        wrap_search: options.wrap_search,
    })
}

fn window_style_colours(
    engine: &MuxEngine,
    config_files: &str,
    options: &zz_mux::TerminalWorkerOptions,
    pane: PaneId,
) -> (Option<TmuxColour>, Option<TmuxColour>) {
    if options.window_style.is_none() && options.window_active_style.is_none() {
        return (None, None);
    }
    let window = engine.state.window_for_pane(pane);
    let session = window
        .and_then(|window| engine.state.windows.get(&window))
        .map(|window| window.session);
    let resolve = |value: &Option<String>| {
        value.as_deref().and_then(|value| {
            let expanded =
                expanded_style_value(engine, config_files, value, session, window, Some(pane));
            zz_protocol::parse_style(&expanded)
        })
    };
    let base = resolve(&options.window_style);
    let active = if options.active_pane {
        resolve(&options.window_active_style)
    } else {
        None
    };
    let fg = active
        .as_ref()
        .and_then(|style| style.fg)
        .or_else(|| base.as_ref().and_then(|style| style.fg));
    let bg = active
        .and_then(|style| style.bg)
        .or_else(|| base.and_then(|style| style.bg));
    (fg, bg)
}

fn pane_terminal_appearance(
    base: &Arc<TerminalAppearance>,
    cursor_style: &str,
    cursor_colour: &str,
    window_colours: (Option<TmuxColour>, Option<TmuxColour>),
    pane_colours: &[(u8, String)],
) -> Arc<TerminalAppearance> {
    let cursor_style = CURSOR_STYLE_OVERRIDES
        .iter()
        .find_map(|(candidate, mapped)| (*candidate == cursor_style).then_some(*mapped))
        .flatten();
    let resolved_color = pane_cursor_color(base, cursor_colour);
    let ground = |colour: Option<TmuxColour>| {
        colour.and_then(|colour| {
            Some((
                appearance_tmux_colour(base, colour)?,
                tmux_colour_class(colour)?,
            ))
        })
    };
    let foreground = ground(window_colours.0);
    let background = ground(window_colours.1);
    let palette = pane_palette(base, pane_colours);
    if cursor_style.is_none()
        && resolved_color.is_none()
        && foreground.is_none()
        && background.is_none()
        && palette.is_none()
    {
        return Arc::clone(base);
    }
    let mut appearance = (**base).clone();
    if let Some((palette, classes)) = palette {
        appearance.palette = palette;
        appearance.palette_classes = classes;
    }
    if let Some((style, blink)) = cursor_style {
        appearance.cursor_style = style;
        appearance.cursor_blink_policy = blink;
    }
    if let Some(color) = resolved_color {
        appearance.cursor_color = color;
    }
    if let Some((color, class)) = foreground {
        appearance.foreground = color;
        appearance.default_classes[0] = Some(class);
    }
    if let Some((color, class)) = background {
        appearance.background = color;
        appearance.default_classes[1] = Some(class);
    }
    Arc::new(appearance)
}

fn tmux_colour_class(colour: TmuxColour) -> Option<ColourClass> {
    match colour {
        TmuxColour::Default | TmuxColour::Terminal => None,
        TmuxColour::Basic(entry) | TmuxColour::Indexed(entry) => Some(ColourClass::Palette(entry)),
        TmuxColour::Rgb(_) | TmuxColour::Theme(_) => Some(ColourClass::Rgb),
    }
}

fn pane_palette(
    base: &TerminalAppearance,
    pane_colours: &[(u8, String)],
) -> Option<(TerminalPalette, Vec<(u8, ColourClass)>)> {
    let mut colors = base.palette.into_array();
    let mut classes = Vec::new();
    for (index, value) in pane_colours {
        let Some((color, class)) = parse_tmux_colour(value).and_then(|colour| {
            Some((
                appearance_tmux_colour(base, colour)?,
                tmux_colour_class(colour)?,
            ))
        }) else {
            continue;
        };
        colors[usize::from(*index)] = color;
        classes.retain(|(entry, _)| entry != index);
        classes.push((*index, class));
    }
    (!classes.is_empty()).then(|| (TerminalPalette::new(colors), classes))
}

fn pane_cursor_color(appearance: &TerminalAppearance, value: &str) -> Option<Color> {
    if let Some(color) = parse_x11_color(value)
        && value.chars().any(char::is_whitespace)
    {
        return Some(color);
    }
    appearance_tmux_colour(appearance, parse_tmux_colour(value)?)
}

fn appearance_tmux_colour(appearance: &TerminalAppearance, colour: TmuxColour) -> Option<Color> {
    match colour {
        TmuxColour::Basic(index) | TmuxColour::Indexed(index) => appearance
            .palette
            .as_array()
            .get(usize::from(index))
            .copied(),
        TmuxColour::Rgb(value) => Some(Color::rgb(
            u8::try_from((value >> 16) & 0xff).expect("red component is one byte"),
            u8::try_from((value >> 8) & 0xff).expect("green component is one byte"),
            u8::try_from(value & 0xff).expect("blue component is one byte"),
        )),
        TmuxColour::Theme(index) => [0, 7, 7, 0, 2, 3, 1, 4, 6, 5]
            .get(usize::from(index))
            .and_then(|index| appearance.palette.as_array().get(*index))
            .copied(),
        TmuxColour::Default | TmuxColour::Terminal => None,
    }
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

fn published_appearance(inner: &ServerState) -> Arc<TerminalAppearance> {
    let styles = inner.engine.copy_mode_style_values();
    if styles == CopyModeStyleValues::default() {
        return Arc::clone(&inner.config.appearance);
    }
    let base = Arc::clone(&inner.config.appearance);
    let mut appearance = (*base).clone();
    let resolve = |value: &str| {
        let expanded = expanded_style_value(
            &inner.engine,
            &inner.config.config_files,
            value,
            None,
            None,
            None,
        );
        zz_protocol::parse_style(&expanded).map(|style| {
            (
                style.fg.and_then(|fg| appearance_tmux_colour(&base, fg)),
                style.bg.and_then(|bg| appearance_tmux_colour(&base, bg)),
            )
        })
    };
    if let Some((fg, bg)) = styles.mode.as_deref().and_then(resolve) {
        if let Some(color) = fg {
            appearance.selection_foreground = color;
        }
        if let Some(color) = bg {
            appearance.selection_background = AppearanceColor::rgba(color.r, color.g, color.b, 255);
        }
    }
    if let Some((fg, bg)) = styles.search_match.as_deref().and_then(resolve)
        && let Some(color) = bg.or(fg)
    {
        appearance.search_match_color = AppearanceColor::rgba(color.r, color.g, color.b, 255);
    }
    if let Some((fg, bg)) = styles.search_current_match.as_deref().and_then(resolve)
        && let Some(color) = bg.or(fg)
    {
        appearance.search_current_color = AppearanceColor::rgba(color.r, color.g, color.b, 255);
    }
    Arc::new(appearance)
}

fn terminal_appearance_updates(
    inner: &ServerState,
) -> Vec<(Arc<TerminalSession>, Arc<TerminalAppearance>)> {
    let mut updates = Vec::with_capacity(
        inner.terminals.len()
            + inner
                .clients
                .values()
                .filter(|c| c.command_output.is_some())
                .count(),
    );
    for (pane, terminal) in inner.terminals.iter() {
        let appearance = terminal_worker_options(
            &inner.engine,
            &inner.config.appearance,
            &inner.config.config_files,
            *pane,
        )
        .map_or_else(
            |_| Arc::clone(&inner.config.appearance),
            |options| options.appearance,
        );
        updates.push((Arc::clone(terminal), appearance));
    }
    for output in inner
        .clients
        .values()
        .filter_map(|c| c.command_output.as_ref())
    {
        let appearance = terminal_worker_options(
            &inner.engine,
            &inner.config.appearance,
            &inner.config.config_files,
            output.pane,
        )
        .map_or_else(
            |_| Arc::clone(&inner.config.appearance),
            |options| options.appearance,
        );
        updates.push((Arc::clone(&output.terminal), appearance));
    }
    updates
}

fn log_appearance_load(reason: &str, load: &AppearanceLoad) {
    log::info!(
        target: "zz_daemon::diagnostics::appearance",
        "loaded appearance reason={reason} root={:?} scheme={} supported={} unsupported={} invalid={} diagnostics_dropped={} bytes={} hash={} appearance={:?}",
        load.root,
        load.appearance.color_scheme.as_str(),
        load.supported,
        load.unsupported,
        load.invalid,
        load.diagnostics_dropped,
        load.bytes_read,
        load.appearance.stable_hash(),
        load.appearance,
    );
    for diagnostic in &load.diagnostics {
        match diagnostic.disposition {
            AppearanceConfigDisposition::Invalid => log::warn!(
                target: "zz_daemon::diagnostics::appearance",
                "config reason={reason} path={} line={} key={:?} disposition={:?} message={}",
                diagnostic.path.display(),
                diagnostic.line,
                diagnostic.key,
                diagnostic.disposition,
                diagnostic.message,
            ),
            _ => log::debug!(
                target: "zz_daemon::diagnostics::appearance",
                "config reason={reason} path={} line={} key={:?} disposition={:?} message={}",
                diagnostic.path.display(),
                diagnostic.line,
                diagnostic.key,
                diagnostic.disposition,
                diagnostic.message,
            ),
        }
    }
}

const MUX_IMPORT_MAX_BYTES: usize = 1024 * 1024;
const MUX_IMPORT_END: &str = "# zz-import-tmux-end: ";

fn read_mux_import_source(path: &Path) -> Result<String, DaemonError> {
    let read = || -> std::io::Result<String> {
        let mut source = String::new();
        fs::File::open(path)?
            .take((MUX_IMPORT_MAX_BYTES + 1) as u64)
            .read_to_string(&mut source)?;
        if source.len() > MUX_IMPORT_MAX_BYTES {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "configuration exceeds the 1 MiB limit",
            ));
        }
        Ok(source)
    };
    read().map_err(|error| {
        std::io::Error::new(error.kind(), format!("{}: {error}", path.display())).into()
    })
}

fn format_config_files(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(",")
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

#[cfg(windows)]
fn accept_connections<T: Transport>(
    listener: &T::Listener,
    shared: &Arc<Shared>,
) -> Result<(), DaemonError> {
    while !shared.stopping.load(Ordering::Acquire) {
        match listener.accept() {
            Ok(stream) => {
                let connection_shared = Arc::clone(shared);
                if let Err(error) = shared.connection_threads.run(Box::new(move || {
                    if let Err(error) = handle_connection(stream, &connection_shared) {
                        log::debug!("client disconnected: {error}");
                    }
                })) {
                    log::warn!("could not start client connection thread: {error}");
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                listener.wait_for_incoming(ACCEPT_WAIT_TIMEOUT)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
struct SocketGuard {
    path: PathBuf,
    armed: bool,
}

#[cfg(windows)]
struct SocketGuard;

impl SocketGuard {
    #[cfg(unix)]
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    #[cfg(windows)]
    fn new(path: PathBuf) -> Self {
        let _ = path;
        Self
    }

    #[cfg(unix)]
    fn release(&mut self) {
        if !std::mem::take(&mut self.armed) {
            return;
        }
        let _ = fs::remove_file(&self.path);
    }

    #[cfg(windows)]
    fn release(&mut self) {}

    #[cfg(windows)]
    fn disarm(&mut self) {}
}

#[cfg(unix)]
impl Drop for SocketGuard {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(unix)]
struct TmuxShimGuard {
    directory: PathBuf,
    executable: PathBuf,
}

#[cfg(unix)]
fn pinned_executable(directory: &Path, executable: &Path) -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let _ = directory;
        let own = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
        if own.exists() {
            return own;
        }
        executable.to_path_buf()
    }
    #[cfg(target_os = "macos")]
    {
        shared_pinned_executable(
            &std::env::temp_dir().join(PINNED_ROOT),
            directory,
            executable,
        )
        .unwrap_or_else(|| executable.to_path_buf())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = directory;
        executable.to_path_buf()
    }
}

#[cfg(target_os = "macos")]
const PINNED_ROOT: &str = "zz-pinned";

#[cfg(target_os = "macos")]
const PINNED_LINK: &str = "pin";

#[cfg(target_os = "macos")]
fn shared_pinned_executable(root: &Path, directory: &Path, executable: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};

    let name = executable.file_name()?;
    let metadata = fs::metadata(executable).ok()?;
    let key = format!(
        "{:x}-{:x}-{:x}-{:x}.{:x}",
        metadata.dev(),
        metadata.ino(),
        metadata.size(),
        metadata.mtime(),
        metadata.mtime_nsec()
    );
    let build = root.join(&key);
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&build)
        .ok()?;
    let pinned = build.join(name);
    if !pinned.exists() {
        let staged = build.join(format!(".{}", std::process::id()));
        let _ = fs::remove_file(&staged);
        clone_file(executable, &staged).ok()?;
        let linked = fs::hard_link(&staged, &pinned);
        let _ = fs::remove_file(&staged);
        if let Err(error) = linked
            && error.kind() != ErrorKind::AlreadyExists
        {
            return None;
        }
    }
    fs::hard_link(&pinned, directory.join(PINNED_LINK)).ok()?;
    remove_unused_pinned_builds(root, &key);
    Some(pinned)
}

#[cfg(target_os = "macos")]
fn remove_unused_pinned_builds(root: &Path, current: &str) {
    use std::os::unix::fs::MetadataExt as _;

    let Ok(builds) = fs::read_dir(root) else {
        return;
    };
    for build in builds.flatten() {
        if build.file_name() == current {
            continue;
        }
        let path = build.path();
        let settled = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                modified
                    .elapsed()
                    .is_ok_and(|age| age > Duration::from_secs(60))
            });
        let referenced = fs::read_dir(&path).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.metadata().is_ok_and(|metadata| metadata.nlink() > 1))
        });
        if settled && !referenced {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "clonefile copies one file the daemon can read into its own directory"
)]
fn clone_file(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;

    unsafe extern "C" {
        fn clonefile(
            src: *const std::ffi::c_char,
            dst: *const std::ffi::c_char,
            flags: u32,
        ) -> std::ffi::c_int;
    }
    let source = std::ffi::CString::new(source.as_os_str().as_bytes())?;
    let target = std::ffi::CString::new(target.as_os_str().as_bytes())?;
    if unsafe { clonefile(source.as_ptr(), target.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn remove_dead_tmux_shims(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let pid = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_prefix("zz-tmux-shim-"))
            .and_then(|rest| rest.split_once('-'))
            .and_then(|(pid, _)| pid.parse::<u32>().ok());
        if pid.is_some_and(|pid| pid != std::process::id() && !crate::pid_alive(pid)) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(unix)]
impl TmuxShimGuard {
    fn install(executable: PathBuf, pin: bool) -> std::io::Result<Self> {
        use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};

        remove_dead_tmux_shims(&std::env::temp_dir());
        let directory = loop {
            let sequence = TMUX_SHIM_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir()
                .join(format!("zz-tmux-shim-{}-{sequence}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            match builder.create(&directory) {
                Ok(()) => break directory,
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        };
        let write = |name: &str, script: &[u8]| -> std::io::Result<()> {
            let mut options = fs::OpenOptions::new();
            options.create_new(true).write(true).mode(0o700);
            let mut file = options.open(directory.join(name))?;
            file.write_all(script)?;
            file.flush()
        };
        if let Err(error) = write("tmux", b"#!/bin/sh\nif [ \"$#\" -eq 0 ] && [ -n \"${TMUX-}\" ]; then\n    printf '%s\\n' 'sessions should be nested with care, unset $TMUX to force' >&2\n    exit 1\nfi\nexec \"$ZZ_TMUX_EXECUTABLE\" \"$@\"\n") {
            let _ = fs::remove_file(directory.join("tmux"));
            let _ = fs::remove_dir(&directory);
            return Err(error);
        }
        let executable = if pin {
            pinned_executable(&directory, &executable)
        } else {
            executable
        };
        if !directory.join("zz").exists()
            && let Err(error) = write("zz", b"#!/bin/sh\nexec \"$ZZ_TMUX_EXECUTABLE\" \"$@\"\n")
        {
            drop(Self {
                directory,
                executable,
            });
            return Err(error);
        }
        Ok(Self {
            directory,
            executable,
        })
    }
}

#[cfg(unix)]
impl Drop for TmuxShimGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        let _ = fs::remove_file(self.directory.join(PINNED_LINK));
        let _ = fs::remove_file(self.directory.join("tmux"));
        let _ = fs::remove_file(self.directory.join("zz"));
        let _ = fs::remove_dir(&self.directory);
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

fn paste_upload_directory(socket_path: &Path) -> PathBuf {
    #[cfg(unix)]
    let root = socket_path
        .parent()
        .map_or_else(|| PathBuf::from("paste"), |parent| parent.join("paste"));
    #[cfg(not(unix))]
    let root = std::env::temp_dir().join("zz").join("paste");
    root.join(
        socket_path
            .file_name()
            .unwrap_or_else(|| OsStr::new("default")),
    )
}

struct OutboundMailbox {
    state: Mutex<OutboundState>,
    ready: Condvar,
    #[cfg(unix)]
    loop_waker: Mutex<Option<(Arc<mio::Waker>, thread::ThreadId)>>,
    terminals_frozen: AtomicBool,
    control: std::sync::OnceLock<shard_sink::ControlFeed>,
}

#[derive(Clone, Debug)]
enum OutboundFrame {
    Owned(Vec<u8>),
    Shared(Arc<[u8]>),
    Grouped {
        encoded: Vec<u8>,
        frames: Vec<OutboundFrame>,
    },
    DeferredGrouped {
        sequence: u64,
        encoded_len: usize,
        frames: Vec<OutboundFrame>,
    },
    #[cfg(unix)]
    Partial {
        frame: Box<OutboundFrame>,
        offset: usize,
    },
}

impl std::ops::Deref for OutboundFrame {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Self::Owned(frame) => frame,
            Self::Shared(frame) => frame,
            Self::Grouped { encoded, .. } => encoded,
            Self::DeferredGrouped { .. } => unreachable!("deferred group has not been collected"),
            #[cfg(unix)]
            Self::Partial { frame, offset } => &frame.as_ref().as_ref()[*offset..],
        }
    }
}

impl AsRef<[u8]> for OutboundFrame {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl From<Vec<u8>> for OutboundFrame {
    fn from(frame: Vec<u8>) -> Self {
        Self::Owned(frame)
    }
}

impl From<Arc<[u8]>> for OutboundFrame {
    fn from(frame: Arc<[u8]>) -> Self {
        Self::Shared(frame)
    }
}

impl OutboundFrame {
    fn is_partial(&self) -> bool {
        #[cfg(unix)]
        {
            matches!(self, Self::Partial { .. })
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::DeferredGrouped { encoded_len, .. } => *encoded_len,
            _ => self.as_ref().len(),
        }
    }

    fn materialize(self) -> Self {
        let Self::DeferredGrouped {
            sequence, frames, ..
        } = self
        else {
            return self;
        };
        let mut encoded = Vec::new();
        zz_protocol::encode_batch_frames_into(sequence, &frames, &mut encoded)
            .expect("admitted deferred group fits its encoded bounds");
        Self::Grouped { encoded, frames }
    }

    #[cfg(test)]
    fn into_vec(self) -> Vec<u8> {
        match self.materialize() {
            Self::Owned(frame) => frame,
            Self::Shared(frame) => frame.to_vec(),
            Self::Grouped { encoded, .. } => encoded,
            Self::DeferredGrouped { .. } => unreachable!("group was materialized"),
            #[cfg(unix)]
            Self::Partial { frame, offset } => frame.as_ref().as_ref()[offset..].to_vec(),
        }
    }
}

#[derive(Default)]
struct OutboundState {
    reliable: VecDeque<OutboundFrame>,
    command_output: Option<PendingCommandOutput>,
    agent: BTreeMap<PaneId, PendingAgent>,
    agent_order: VecDeque<PaneId>,
    terminals: BTreeMap<PaneId, PendingTerminal>,
    delivered_terminals: BTreeMap<PaneId, TerminalGeneration>,
    delivered_images: BTreeMap<PaneId, BTreeMap<u32, u64>>,
    delivered_pasted_images: BTreeMap<PaneId, BTreeMap<u32, u64>>,
    terminal_order: VecDeque<PaneId>,
    preview_refreshes: BTreeSet<PaneId>,
    preview_refresh_order: VecDeque<PaneId>,
    recycled_frames: Vec<Vec<u8>>,
    recycled_capacity: usize,
    queued_bytes: usize,
    written_bytes: u64,
    discarded_bytes: u64,
    closed: bool,
    writer_finished: bool,
    writer_inflight_bytes: usize,
    writer_inflight_messages: usize,
    writer_batch_reliable: usize,
    #[cfg(unix)]
    quiet_socket: Option<std::os::fd::OwnedFd>,
    #[cfg(unix)]
    direct_socket: Option<std::os::fd::OwnedFd>,
    terminals_held: bool,
    attach_batch: bool,
    #[cfg(unix)]
    attach_rounds: u64,
    #[cfg(unix)]
    attach_settling: Option<AttachSettling>,
    buffered: bool,
    ctrl_collecting: ControlCollection,
    #[cfg(unix)]
    flow: Option<flow::FlowWindow>,
    client_capabilities: Vec<&'static str>,
}

#[cfg(unix)]
struct AttachSettling {
    round: u64,
    pending: usize,
    flush: Option<bool>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ControlCollection {
    #[default]
    None,
    Attach,
    Quiet,
}

#[cfg(any(windows, test))]
struct OutboundWriterGuard<'a>(&'a OutboundMailbox);

#[cfg(any(windows, test))]
impl Drop for OutboundWriterGuard<'_> {
    fn drop(&mut self) {
        self.0.mark_writer_finished();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalEnqueue {
    Queued,
    NeedsFull,
    Dropped,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KittyImageEnqueue {
    Queued,
    AlreadyDelivered,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PastedImageEnqueue {
    Queued,
    AlreadyDelivered,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalGeneration {
    content: u64,
    view: u64,
    dictionary: u32,
    columns: u16,
    rows: u16,
}

impl TerminalGeneration {
    fn precedes(self, other: Self) -> bool {
        self.view < other.view && self.content <= other.content
    }
}

mod attach;

#[cfg(test)]
mod attach_tests;

fn newer_terminal_delivered(
    state: &OutboundState,
    pane: PaneId,
    current: TerminalGeneration,
) -> bool {
    state
        .terminals
        .get(&pane)
        .is_some_and(|pending| current.precedes(pending.current))
        || state
            .delivered_terminals
            .get(&pane)
            .is_some_and(|delivered| current.precedes(*delivered))
}

fn terminal_update_redundant(
    state: &OutboundState,
    pane: PaneId,
    transition: TerminalTransition,
    preview: bool,
) -> bool {
    let current = transition.current;
    transition.base.is_none() && newer_terminal_delivered(state, pane, current)
        || state.terminals.get(&pane).is_some_and(|pending| {
            pending.current == current
                && (preview || !pending.preview)
                && (pending.full || transition.base.is_some())
        })
        || state.delivered_terminals.get(&pane) == Some(&current)
}

fn forget_delivered_terminal_state(state: &mut OutboundState, pane: PaneId) {
    state.delivered_terminals.remove(&pane);
    if state
        .terminals
        .get(&pane)
        .is_some_and(|pending| !pending.full)
    {
        remove_pending_terminal(state, pane);
    }
}

fn forget_delivered_terminals_state(state: &mut OutboundState) {
    state.delivered_terminals.clear();
    let patches = state
        .terminals
        .iter()
        .filter_map(|(pane, pending)| (!pending.full).then_some(*pane))
        .collect::<Vec<_>>();
    for pane in patches {
        remove_pending_terminal(state, pane);
    }
}

struct PendingTerminal {
    encoded: Arc<[u8]>,
    current: TerminalGeneration,
    preview: bool,
    full: bool,
}

struct PendingCommandOutput {
    output_id: u64,
    encoded: Vec<u8>,
}

#[derive(Default)]
struct PendingAgent {
    frames: VecDeque<(u64, OutboundFrame)>,
    bytes: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct TerminalTransition {
    base: Option<TerminalGeneration>,
    current: TerminalGeneration,
}

struct PaneFrameFanout {
    diff: TerminalDiffScratch,
    tail: PatchTail,
    scratch: Vec<u8>,
    patches: Vec<(TerminalTransition, Arc<[u8]>)>,
}

impl PaneFrameFanout {
    fn new() -> Self {
        Self {
            diff: TerminalDiffScratch::default(),
            tail: PatchTail::default(),
            scratch: Vec::new(),
            patches: Vec::new(),
        }
    }

    fn release(&mut self) {
        self.diff.release_shared();
        self.patches.clear();
    }

    fn enqueue(
        &mut self,
        subscriber: &OutboundMailbox,
        frames: &TerminalFrames,
        pane: PaneId,
        base: Option<&TerminalViewport>,
        current: &TerminalViewport,
        delivery: TerminalDelivery,
    ) -> TerminalEnqueue {
        let Some(base) = base else {
            return subscriber.enqueue_terminal_viewport(pane, current, delivery, frames);
        };
        let transition = TerminalTransition {
            base: Some(viewport_generation(base)),
            current: viewport_generation(current),
        };
        if let Some((_, frame)) = self.patches.iter().find(|(seen, _)| *seen == transition) {
            let frame = Arc::clone(frame);
            return subscriber.enqueue_terminal_with(pane, transition, delivery, || Ok(frame));
        }
        let Self {
            diff,
            tail,
            scratch,
            patches,
        } = self;
        let Some(patch) = TerminalViewport::diff_shared(base, current, diff) else {
            return subscriber.enqueue_terminal_viewport(pane, current, delivery, frames);
        };
        subscriber.enqueue_terminal_with(pane, transition, delivery, || {
            let frame = encode_terminal_patch_frame(pane, &patch, tail, scratch)?;
            patches.push((transition, Arc::clone(&frame)));
            Ok(frame)
        })
    }
}

const MAX_CACHED_FULL_FRAMES: usize = 16;
const MAX_CACHED_FULL_FRAME_BYTES: usize = 1024 * 1024;

struct CachedFullFrame {
    pane: PaneId,
    generation: TerminalGeneration,
    cells: usize,
    frame: Arc<[u8]>,
}

#[derive(Default)]
struct CachedFullFrames {
    frames: VecDeque<CachedFullFrame>,
    bytes: usize,
}

#[derive(Default)]
struct TerminalFrames {
    full: Mutex<CachedFullFrames>,
}

impl TerminalFrames {
    fn full(&self, pane: PaneId, viewport: &TerminalViewport) -> Result<Arc<[u8]>, ProtocolError> {
        let generation = viewport_generation(viewport);
        let cells = Arc::as_ptr(&viewport.cells).cast::<u8>().addr();
        let cached = self.full.lock().frames.iter().find_map(|cached| {
            (cached.pane == pane && cached.generation == generation && cached.cells == cells)
                .then(|| Arc::clone(&cached.frame))
        });
        if let Some(frame) = cached {
            return Ok(frame);
        }
        let frame = encode_terminal_viewport_frame(pane, viewport)?;
        let mut full = self.full.lock();
        let CachedFullFrames { frames, bytes } = &mut *full;
        if let Some(index) = frames.iter().position(|cached| cached.pane == pane)
            && let Some(replaced) = frames.remove(index)
        {
            *bytes -= replaced.frame.len();
        }
        if frame.len() > MAX_CACHED_FULL_FRAME_BYTES {
            return Ok(frame);
        }
        while frames.len() >= MAX_CACHED_FULL_FRAMES
            || *bytes + frame.len() > MAX_CACHED_FULL_FRAME_BYTES
        {
            let Some(evicted) = frames.pop_front() else {
                break;
            };
            *bytes -= evicted.frame.len();
        }
        *bytes += frame.len();
        frames.push_back(CachedFullFrame {
            pane,
            generation,
            cells,
            frame: Arc::clone(&frame),
        });
        Ok(frame)
    }
}

#[cfg(test)]
thread_local! {
    static TERMINAL_ENCODES: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

#[cfg(test)]
fn terminal_encodes() -> (usize, usize) {
    TERMINAL_ENCODES.with(std::cell::Cell::get)
}

fn terminal_stream_sequence(viewport: &TerminalViewport) -> u64 {
    viewport.view_generation
}

fn encode_terminal_patch_frame(
    pane: PaneId,
    patch: &TerminalPatchRef<'_>,
    tail: &mut PatchTail,
    scratch: &mut Vec<u8>,
) -> Result<Arc<[u8]>, ProtocolError> {
    #[cfg(test)]
    TERMINAL_ENCODES.with(|count| count.update(|(patches, fulls)| (patches + 1, fulls)));
    encode_terminal_patch_event_into(
        pane,
        terminal_stream_sequence(patch.current),
        patch,
        tail,
        scratch,
    )?;
    Ok(Arc::from(scratch.as_slice()))
}

fn encode_terminal_viewport_frame(
    pane: PaneId,
    viewport: &TerminalViewport,
) -> Result<Arc<[u8]>, ProtocolError> {
    #[cfg(test)]
    TERMINAL_ENCODES.with(|count| count.update(|(patches, fulls)| (patches, fulls + 1)));
    let mut frame = Vec::new();
    encode_terminal_viewport_event_into(
        pane,
        terminal_stream_sequence(viewport),
        viewport,
        &mut frame,
    )?;
    Ok(frame.into())
}

#[cfg(test)]
mod pane_frame_tests;

#[cfg(test)]
mod encode_once_tests;

mod shard_sink;

#[cfg(test)]
mod shard_sink_tests;

#[cfg(all(test, unix))]
mod control_sink_tests;

#[derive(Clone, Copy)]
enum TerminalDelivery {
    Foreground,
    Preview { foreground_panes: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalGeometry {
    columns: u16,
    rows: u16,
    cell_width_px: u32,
    cell_height_px: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalStreamKind {
    Foreground,
    Preview,
}

fn viewport_generation(viewport: &TerminalViewport) -> TerminalGeneration {
    TerminalGeneration {
        content: viewport.generation,
        view: viewport.view_generation,
        dictionary: viewport.dictionary_generation,
        columns: viewport.columns,
        rows: viewport.rows,
    }
}

#[cfg(test)]
fn patch_transition(patch: &zz_terminal::TerminalViewportPatch) -> TerminalTransition {
    TerminalTransition {
        base: Some(TerminalGeneration {
            content: patch.base_generation,
            view: patch.base_view_generation,
            dictionary: patch.dictionary_generation,
            columns: patch.columns,
            rows: patch.rows,
        }),
        current: TerminalGeneration {
            content: patch.generation,
            view: patch.view_generation,
            dictionary: patch.dictionary_generation,
            columns: patch.columns,
            rows: patch.rows,
        },
    }
}

#[cfg(test)]
fn terminal_transition(pane: PaneId, message: &ProtocolMessage) -> Option<TerminalTransition> {
    let ProtocolMessage::Event(Event { payload, .. }) = message else {
        return None;
    };
    match payload {
        EventPayload::TerminalViewport {
            pane: target,
            viewport,
        } if *target == pane => Some(TerminalTransition {
            base: None,
            current: viewport_generation(viewport),
        }),
        EventPayload::TerminalPatch {
            pane: target,
            patch,
        } if *target == pane => Some(patch_transition(patch)),
        _ => None,
    }
}

impl OutboundMailbox {
    fn notify_one(&self) {
        #[cfg(unix)]
        if let Some((waker, owner)) = self.loop_waker.lock().as_ref() {
            if *owner != thread::current().id() && !shard_sink::hold_loop_wake(waker) {
                let _ = waker.wake();
            }
            return;
        }
        self.ready.notify_one();
    }

    fn notify_all(&self) {
        self.ready.notify_all();
        #[cfg(unix)]
        if let Some((waker, owner)) = self.loop_waker.lock().as_ref()
            && *owner != thread::current().id()
        {
            let _ = waker.wake();
        }
    }

    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(OutboundState::default()),
            ready: Condvar::new(),
            #[cfg(unix)]
            loop_waker: Mutex::new(None),
            terminals_frozen: AtomicBool::new(false),
            control: std::sync::OnceLock::new(),
        })
    }

    fn control_feed(&self) -> Option<&shard_sink::ControlFeed> {
        self.control.get()
    }

    fn control_ordered(
        &self,
        order: shard_sink::ControlOrder,
        line: impl FnOnce(&OutboundMailbox) -> bool + Send + 'static,
    ) -> bool {
        match self.control_feed() {
            Some(feed) => feed.order(self, order, line),
            None => line(self),
        }
    }

    fn ensure_control_feed(&self, wake: &Arc<shard_sink::ControlWake>) -> &shard_sink::ControlFeed {
        self.control
            .get_or_init(|| shard_sink::ControlFeed::new(Arc::clone(wake)))
    }

    fn terminals_frozen(&self) -> bool {
        self.terminals_frozen.load(Ordering::Acquire)
    }

    fn is_closed(&self) -> bool {
        self.state.lock().closed
    }

    fn terminal_pending(&self, pane: PaneId) -> bool {
        self.state.lock().terminals.contains_key(&pane)
    }

    fn encode_message(&self, message: &ProtocolMessage) -> Result<Vec<u8>, ProtocolError> {
        self.encode_with(|frame| encode_protocol_message_into(message, frame))
    }

    fn encode_with(
        &self,
        encode: impl FnOnce(&mut Vec<u8>) -> Result<(), ProtocolError>,
    ) -> Result<Vec<u8>, ProtocolError> {
        let mut frame = {
            let mut state = self.state.lock();
            take_recycled_frame(&mut state)
        };
        if let Err(error) = encode(&mut frame) {
            self.recycle_frame(frame);
            return Err(error);
        }
        Ok(frame)
    }

    fn recycle_frame(&self, frame: Vec<u8>) {
        let mut state = self.state.lock();
        recycle_outbound_frame(&mut state, frame);
    }

    fn stats(&self) -> (u64, u64) {
        let state = self.state.lock();
        (state.written_bytes, state.discarded_bytes)
    }

    fn accept_client_capabilities(&self, declared: &[String]) {
        self.state.lock().client_capabilities = zz_protocol::CLIENT_CAPABILITY_NAMES
            .iter()
            .chain(zz_protocol::OPT_IN_CLIENT_CAPABILITY_NAMES)
            .copied()
            .filter(|name| declared.iter().any(|capability| capability == name))
            .collect();
    }

    #[cfg(all(feature = "agent", unix))]
    fn has_client_capability(&self, capability: &str) -> bool {
        self.state.lock().client_capabilities.contains(&capability)
    }

    #[must_use]
    fn enqueue_reliable(&self, message: &ProtocolMessage) -> bool {
        self.enqueue_reliable_with_wakeup(message, true)
    }

    #[must_use]
    fn enqueue_reliable_with_wakeup(&self, message: &ProtocolMessage, wakeup: bool) -> bool {
        let encoded = match self.encode_message(message) {
            Ok(encoded) => encoded,
            Err(error) => {
                log::error!(
                    target: "zz_daemon::diagnostics::outbound",
                    "failed to encode outbound control message: {error}"
                );
                return false;
            }
        };
        self.enqueue_reliable_frame(message, encoded.into(), wakeup)
    }

    fn enqueue_reliable_frame(
        &self,
        message: &ProtocolMessage,
        encoded: OutboundFrame,
        wakeup: bool,
    ) -> bool {
        let order = shard_sink::control_order(message);
        let removed_pane = match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::PaneRemoved(pane),
                ..
            }) => Some(*pane),
            _ => None,
        };
        let removed_images = match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::KittyImagesRemoved { pane, image_ids },
                ..
            }) => Some((*pane, image_ids.as_slice())),
            _ => None,
        };
        let closes_command_output = match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::CommandOutput {
                        output_id,
                        viewport: None,
                        ..
                    },
                ..
            }) => Some(*output_id),
            _ => None,
        };
        let mut state = self.state.lock();
        if state.closed {
            return false;
        }
        if let Some(capability) = message.required_capability()
            && !state.client_capabilities.contains(&capability)
        {
            return true;
        }
        if let Some(pane) = removed_pane {
            remove_pending_terminal(&mut state, pane);
            clear_preview_refresh(&mut state, pane);
            clear_pending_agent(&mut state, pane);
            state.delivered_terminals.remove(&pane);
            state.delivered_images.remove(&pane);
            state.delivered_pasted_images.remove(&pane);
        }
        if let Some((pane, image_ids)) = removed_images
            && let Some(delivered) = state.delivered_images.get_mut(&pane)
        {
            for image_id in image_ids {
                delivered.remove(image_id);
            }
            if delivered.is_empty() {
                state.delivered_images.remove(&pane);
            }
        }
        if let Some(output_id) = closes_command_output
            && state
                .command_output
                .as_ref()
                .is_some_and(|pending| output_id == 0 || pending.output_id <= output_id)
        {
            let pending = state
                .command_output
                .take()
                .expect("matching command output is pending");
            state.queued_bytes = state.queued_bytes.saturating_sub(pending.encoded.len());
            discard_outbound_frame(&mut state, pending.encoded);
        }
        if let Some(order) = order
            && let Some(feed) = self.control_feed()
        {
            drop(state);
            return feed.order(self, order, move |mailbox| {
                mailbox.push_reliable(mailbox.state.lock(), encoded, wakeup)
            });
        }
        self.push_reliable(state, encoded, wakeup)
    }

    fn push_reliable(
        &self,
        mut state: parking_lot::MutexGuard<'_, OutboundState>,
        encoded: OutboundFrame,
        wakeup: bool,
    ) -> bool {
        if state.closed {
            return false;
        }
        if state.attach_batch && state.reliable.len() >= MAX_RELIABLE_MESSAGES / 2 {
            state.attach_batch = false;
        }
        if !reserve_outbound_bytes(&mut state, encoded.len(), 0) {
            shed_preview_terminals(&mut state);
        }
        if (!state.buffered
            && state
                .reliable
                .len()
                .saturating_add(state.writer_inflight_messages)
                >= MAX_RELIABLE_MESSAGES)
            || !reserve_outbound_bytes(&mut state, encoded.len(), 0)
        {
            close_outbound_too_far_behind(&mut state);
            self.notify_all();
            return false;
        }
        state.queued_bytes += encoded.len();
        state.reliable.push_back(encoded);
        let wakeup = wakeup && state.ctrl_collecting == ControlCollection::None;
        drop(state);
        if wakeup {
            self.notify_one();
        }
        true
    }

    #[must_use]
    fn enqueue_encoded_reliable(&self, encoded: impl Into<OutboundFrame>) -> bool {
        let encoded = encoded.into();
        self.control_ordered(shard_sink::ControlOrder::AfterOutput, move |mailbox| {
            mailbox.enqueue_encoded_reliable_with(encoded, |_| {})
        })
    }

    #[must_use]
    fn enqueue_attached(&self, message: &ProtocolMessage) -> bool {
        let encoded = match self.encode_message(message) {
            Ok(encoded) => encoded,
            Err(error) => {
                log::error!(
                    target: "zz_daemon::diagnostics::outbound",
                    "failed to encode outbound attach message: {error}"
                );
                return false;
            }
        };
        self.control_ordered(shard_sink::ControlOrder::AfterOutput, move |mailbox| {
            mailbox.enqueue_encoded_reliable_with(encoded.into(), |state| {
                forget_delivered_terminals_state(state);
                state.terminals_held = false;
            })
        })
    }

    fn hold_terminals(&self) {
        let mut state = self.state.lock();
        state.terminals_held = true;
        state.attach_batch = true;
    }

    fn release_terminals(&self) {
        let mut state = self.state.lock();
        let held = std::mem::take(&mut state.terminals_held);
        if !std::mem::take(&mut state.attach_batch) && !held {
            return;
        }
        drop(state);
        self.notify_one();
    }

    fn enqueue_encoded_reliable_with(
        &self,
        encoded: OutboundFrame,
        before_push: impl FnOnce(&mut OutboundState),
    ) -> bool {
        self.enqueue_encoded_reliable_with_locked(self.state.lock(), encoded, before_push)
    }

    fn enqueue_encoded_reliable_with_locked(
        &self,
        mut state: parking_lot::MutexGuard<'_, OutboundState>,
        encoded: OutboundFrame,
        before_push: impl FnOnce(&mut OutboundState),
    ) -> bool {
        if state.closed {
            return false;
        }
        if state.attach_batch && state.reliable.len() >= MAX_RELIABLE_MESSAGES / 2 {
            state.attach_batch = false;
        }
        if !reserve_outbound_bytes(&mut state, encoded.len(), 0) {
            shed_preview_terminals(&mut state);
        }
        if (!state.buffered
            && state
                .reliable
                .len()
                .saturating_add(state.writer_inflight_messages)
                >= MAX_RELIABLE_MESSAGES)
            || !reserve_outbound_bytes(&mut state, encoded.len(), 0)
        {
            close_outbound_too_far_behind(&mut state);
            self.notify_all();
            return false;
        }
        before_push(&mut state);
        state.queued_bytes += encoded.len();
        state.reliable.push_back(encoded);
        let wakeup = state.ctrl_collecting == ControlCollection::None;
        drop(state);
        if wakeup {
            self.notify_one();
        }
        true
    }

    fn forget_delivered_terminal(&self, pane: PaneId) {
        forget_delivered_terminal_state(&mut self.state.lock(), pane);
    }

    fn forget_delivered_terminals(&self) {
        forget_delivered_terminals_state(&mut self.state.lock());
    }

    fn enqueue_kitty_image(
        &self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
        frames: &[Vec<u8>],
    ) -> KittyImageEnqueue {
        let mut state = self.state.lock();
        if state.closed {
            return KittyImageEnqueue::Closed;
        }
        if state
            .delivered_images
            .get(&pane)
            .and_then(|images| images.get(&image_id))
            == Some(&generation)
        {
            return KittyImageEnqueue::AlreadyDelivered;
        }
        let Some(frame_bytes) = frames
            .iter()
            .try_fold(0_usize, |total, frame| total.checked_add(frame.len()))
        else {
            close_outbound(&mut state);
            self.notify_all();
            return KittyImageEnqueue::Closed;
        };
        if !reserve_outbound_bytes(&mut state, frame_bytes, 0) {
            shed_preview_terminals(&mut state);
        }
        if state
            .reliable
            .len()
            .saturating_add(state.writer_inflight_messages)
            .saturating_add(frames.len())
            > MAX_RELIABLE_MESSAGES
            || !reserve_outbound_bytes(&mut state, frame_bytes, 0)
        {
            close_outbound(&mut state);
            self.notify_all();
            return KittyImageEnqueue::Closed;
        }
        for frame in frames {
            state.queued_bytes += frame.len();
            state.reliable.push_back(frame.clone().into());
        }
        state
            .delivered_images
            .entry(pane)
            .or_default()
            .insert(image_id, generation);
        drop(state);
        self.notify_one();
        KittyImageEnqueue::Queued
    }

    #[cfg(test)]
    fn enqueue_kitty_images_removed(&self, pane: PaneId, image_ids: &[u32]) {
        let image_ids = {
            let state = self.state.lock();
            let Some(delivered) = state.delivered_images.get(&pane) else {
                return;
            };
            image_ids
                .iter()
                .copied()
                .filter(|image_id| delivered.contains_key(image_id))
                .collect::<Vec<_>>()
        };
        for image_ids in image_ids.chunks(zz_protocol::MAX_KITTY_IMAGE_REMOVALS) {
            let message = ProtocolMessage::Event(Event {
                sequence: Shared::next_sequence(),
                payload: EventPayload::KittyImagesRemoved {
                    pane,
                    image_ids: image_ids.to_vec(),
                },
            });
            if !self.enqueue_reliable(&message) {
                break;
            }
        }
    }

    fn enqueue_kitty_image_removed_if_generation(
        &self,
        pane: PaneId,
        image_id: u32,
        generation: u64,
    ) {
        let message = ProtocolMessage::Event(Event {
            sequence: Shared::next_sequence(),
            payload: EventPayload::KittyImagesRemoved {
                pane,
                image_ids: vec![image_id],
            },
        });
        let Ok(encoded) = self.encode_message(&message) else {
            return;
        };
        let state = self.state.lock();
        if state
            .delivered_images
            .get(&pane)
            .and_then(|images| images.get(&image_id))
            != Some(&generation)
        {
            return;
        }
        self.enqueue_encoded_reliable_with_locked(state, encoded.into(), |state| {
            if let Some(images) = state.delivered_images.get_mut(&pane) {
                images.remove(&image_id);
                if images.is_empty() {
                    state.delivered_images.remove(&pane);
                }
            }
        });
    }

    fn enqueue_pasted_image(
        &self,
        pane: PaneId,
        number: u32,
        token: u64,
        frames: &[Vec<u8>],
    ) -> PastedImageEnqueue {
        let mut state = self.state.lock();
        if state.closed {
            return PastedImageEnqueue::Closed;
        }
        if state
            .delivered_pasted_images
            .get(&pane)
            .and_then(|images| images.get(&number))
            == Some(&token)
        {
            return PastedImageEnqueue::AlreadyDelivered;
        }
        let Some(frame_bytes) = frames
            .iter()
            .try_fold(0_usize, |total, frame| total.checked_add(frame.len()))
        else {
            close_outbound(&mut state);
            self.notify_all();
            return PastedImageEnqueue::Closed;
        };
        if !reserve_outbound_bytes(&mut state, frame_bytes, 0) {
            shed_preview_terminals(&mut state);
        }
        if state
            .reliable
            .len()
            .saturating_add(state.writer_inflight_messages)
            .saturating_add(frames.len())
            > MAX_RELIABLE_MESSAGES
            || !reserve_outbound_bytes(&mut state, frame_bytes, 0)
        {
            close_outbound(&mut state);
            self.notify_all();
            return PastedImageEnqueue::Closed;
        }
        for frame in frames {
            state.queued_bytes += frame.len();
            state.reliable.push_back(frame.clone().into());
        }
        state
            .delivered_pasted_images
            .entry(pane)
            .or_default()
            .insert(number, token);
        drop(state);
        self.notify_one();
        PastedImageEnqueue::Queued
    }

    #[cfg(feature = "agent")]
    fn enqueue_agent(
        &self,
        pane: PaneId,
        first_seq: u64,
        encoded: impl Into<OutboundFrame>,
    ) -> bool {
        let encoded = encoded.into();
        let lagged_from = {
            let mut state = self.state.lock();
            if state.closed {
                return false;
            }
            let (queued_bytes, queued_first) =
                state.agent.get(&pane).map_or((0, first_seq), |queued| {
                    (
                        queued.bytes,
                        queued.frames.front().map_or(first_seq, |(seq, _)| *seq),
                    )
                });
            let exceeds_lane = queued_bytes.saturating_add(encoded.len()) > MAX_PENDING_AGENT_BYTES;
            if !exceeds_lane && !reserve_outbound_bytes(&mut state, encoded.len(), 0) {
                shed_preview_terminals(&mut state);
            }
            if exceeds_lane || !reserve_outbound_bytes(&mut state, encoded.len(), 0) {
                clear_pending_agent(&mut state, pane);
                Some(queued_first)
            } else {
                let fresh = queued_bytes == 0;
                state.queued_bytes += encoded.len();
                let queued = state.agent.entry(pane).or_default();
                queued.bytes = queued.bytes.saturating_add(encoded.len());
                queued.frames.push_back((first_seq, encoded));
                if fresh {
                    state.agent_order.push_back(pane);
                }
                None
            }
        };
        let Some(next_seq) = lagged_from else {
            self.notify_one();
            return true;
        };
        log::warn!(
            target: "zz_daemon::diagnostics::outbound",
            "agent lane for {pane} overflowed; asking for a replay from {next_seq}",
        );
        self.enqueue_reliable(&Shared::event(EventPayload::AgentLagged { pane, next_seq }))
    }

    #[cfg(feature = "agent")]
    fn enqueue_agent_replay(&self, pane: PaneId, frames: Vec<(u64, Vec<u8>)>) -> bool {
        self.enqueue_shared_agent_replay(
            pane,
            frames
                .into_iter()
                .map(|(sequence, frame)| (sequence, frame.into()))
                .collect(),
        )
    }

    #[cfg(feature = "agent")]
    fn enqueue_shared_agent_replay(&self, pane: PaneId, frames: Vec<(u64, OutboundFrame)>) -> bool {
        let Some(bytes) = frames
            .iter()
            .try_fold(0_usize, |total, (_, frame)| total.checked_add(frame.len()))
        else {
            return false;
        };
        let mut state = self.state.lock();
        if state.closed {
            return false;
        }
        let replaced = state.agent.get(&pane).map_or(0, |queued| queued.bytes);
        if bytes <= MAX_PENDING_AGENT_REPLAY_BYTES
            && !reserve_outbound_bytes(&mut state, bytes, replaced)
        {
            shed_preview_terminals(&mut state);
        }
        if bytes > MAX_PENDING_AGENT_REPLAY_BYTES
            || !reserve_outbound_bytes(&mut state, bytes, replaced)
        {
            close_outbound(&mut state);
            drop(state);
            self.notify_all();
            return false;
        }
        clear_pending_agent(&mut state, pane);
        if frames.is_empty() {
            return true;
        }
        state.queued_bytes = state.queued_bytes.saturating_add(bytes);
        let queued = state.agent.entry(pane).or_default();
        queued.bytes = bytes;
        queued.frames.extend(frames);
        state.agent_order.push_back(pane);
        drop(state);
        self.notify_one();
        true
    }

    #[cfg(feature = "agent")]
    fn cancel_agent(&self, pane: PaneId) {
        let mut state = self.state.lock();
        clear_pending_agent(&mut state, pane);
    }

    #[cfg(test)]
    fn enqueue_terminal(&self, pane: PaneId, message: &ProtocolMessage) -> TerminalEnqueue {
        let Some(transition) = terminal_transition(pane, message) else {
            log::error!("refusing a non-terminal update in the terminal mailbox for {pane}");
            return TerminalEnqueue::Closed;
        };
        self.enqueue_terminal_with(pane, transition, TerminalDelivery::Foreground, || {
            zz_protocol::encode_protocol_message(message).map(Arc::from)
        })
    }

    #[cfg(test)]
    fn enqueue_terminal_preview(
        &self,
        pane: PaneId,
        message: &ProtocolMessage,
        foreground_panes: usize,
    ) -> TerminalEnqueue {
        let Some(transition) = terminal_transition(pane, message) else {
            log::error!("refusing a non-terminal preview in the terminal mailbox for {pane}");
            return TerminalEnqueue::Closed;
        };
        self.enqueue_terminal_with(
            pane,
            transition,
            TerminalDelivery::Preview { foreground_panes },
            || zz_protocol::encode_protocol_message(message).map(Arc::from),
        )
    }

    fn enqueue_terminal_viewport(
        &self,
        pane: PaneId,
        viewport: &TerminalViewport,
        delivery: TerminalDelivery,
        frames: &TerminalFrames,
    ) -> TerminalEnqueue {
        self.enqueue_terminal_with(
            pane,
            TerminalTransition {
                base: None,
                current: viewport_generation(viewport),
            },
            delivery,
            || frames.full(pane, viewport),
        )
    }

    fn enqueue_terminal_with(
        &self,
        pane: PaneId,
        transition: TerminalTransition,
        delivery: TerminalDelivery,
        encode: impl FnOnce() -> Result<Arc<[u8]>, ProtocolError>,
    ) -> TerminalEnqueue {
        {
            let mut state = self.state.lock();
            if state.closed {
                return TerminalEnqueue::Closed;
            }
            if matches!(delivery, TerminalDelivery::Foreground) {
                clear_preview_refresh(&mut state, pane);
            }
            let preview = matches!(delivery, TerminalDelivery::Preview { .. });
            if terminal_update_redundant(&state, pane, transition, preview) {
                return TerminalEnqueue::Dropped;
            }
            if state.terminals.contains_key(&pane) {
                return match delivery {
                    TerminalDelivery::Foreground => TerminalEnqueue::NeedsFull,
                    TerminalDelivery::Preview { .. } => {
                        mark_preview_refresh(&mut state, pane);
                        TerminalEnqueue::Dropped
                    }
                };
            }
            if transition.base.is_some()
                && transition.base != state.delivered_terminals.get(&pane).copied()
            {
                return TerminalEnqueue::NeedsFull;
            }
            if let TerminalDelivery::Preview { foreground_panes } = delivery
                && (state.terminals.len() >= MAX_PENDING_TERMINALS.saturating_sub(foreground_panes)
                    || state.queued_bytes >= MAX_PREVIEW_OUTBOUND_BYTES)
            {
                if foreground_panes < MAX_PENDING_TERMINALS {
                    mark_preview_refresh(&mut state, pane);
                }
                return TerminalEnqueue::Dropped;
            }
        }
        let Ok(encoded) = encode() else {
            log::error!("failed to encode terminal update for {pane}");
            return TerminalEnqueue::Closed;
        };
        #[cfg(unix)]
        let off_loop = self
            .loop_waker
            .lock()
            .as_ref()
            .is_none_or(|(_, owner)| *owner != thread::current().id());
        let mut state = self.state.lock();
        if state.closed {
            return TerminalEnqueue::Closed;
        }
        if matches!(delivery, TerminalDelivery::Foreground) {
            clear_preview_refresh(&mut state, pane);
        }
        let preview = matches!(delivery, TerminalDelivery::Preview { .. });
        if terminal_update_redundant(&state, pane, transition, preview) {
            return TerminalEnqueue::Dropped;
        }
        if state.terminals.contains_key(&pane) {
            return match delivery {
                TerminalDelivery::Foreground => TerminalEnqueue::NeedsFull,
                TerminalDelivery::Preview { .. } => {
                    mark_preview_refresh(&mut state, pane);
                    TerminalEnqueue::Dropped
                }
            };
        }
        if transition.base.is_some()
            && transition.base != state.delivered_terminals.get(&pane).copied()
        {
            return TerminalEnqueue::NeedsFull;
        }
        match delivery {
            TerminalDelivery::Foreground => {
                if state.terminals.len() >= MAX_PENDING_TERMINALS
                    || !reserve_outbound_bytes(&mut state, encoded.len(), 0)
                {
                    shed_preview_terminals(&mut state);
                }
                if state.terminals.len() >= MAX_PENDING_TERMINALS
                    || !reserve_outbound_bytes(&mut state, encoded.len(), 0)
                {
                    close_outbound(&mut state);
                    self.notify_all();
                    return TerminalEnqueue::Closed;
                }
            }
            TerminalDelivery::Preview { foreground_panes } => {
                if state.terminals.len() >= MAX_PENDING_TERMINALS.saturating_sub(foreground_panes)
                    || state
                        .queued_bytes
                        .checked_add(encoded.len())
                        .is_none_or(|total| total > MAX_PREVIEW_OUTBOUND_BYTES)
                {
                    if foreground_panes < MAX_PENDING_TERMINALS
                        && encoded.len() <= MAX_PREVIEW_OUTBOUND_BYTES
                    {
                        mark_preview_refresh(&mut state, pane);
                    }
                    return TerminalEnqueue::Dropped;
                }
            }
        }
        if matches!(delivery, TerminalDelivery::Preview { .. }) {
            clear_preview_refresh(&mut state, pane);
        }
        #[cfg(unix)]
        if off_loop && let Some(written) = write_direct_terminal(&mut state, &encoded) {
            state.delivered_terminals.insert(pane, transition.current);
            if written == encoded.len() {
                return TerminalEnqueue::Queued;
            }
            state.queued_bytes += encoded.len() - written;
            state.reliable.push_back(OutboundFrame::Partial {
                frame: Box::new(OutboundFrame::Shared(encoded)),
                offset: written,
            });
            drop(state);
            self.notify_one();
            return TerminalEnqueue::Queued;
        }
        let idle = state.terminals.is_empty();
        state.queued_bytes += encoded.len();
        state.terminals.insert(
            pane,
            PendingTerminal {
                encoded,
                current: transition.current,
                preview: matches!(delivery, TerminalDelivery::Preview { .. }),
                full: transition.base.is_none(),
            },
        );
        state.terminal_order.push_back(pane);
        drop(state);
        if idle {
            self.notify_one();
        }
        TerminalEnqueue::Queued
    }

    #[cfg(test)]
    fn replace_terminal(&self, pane: PaneId, message: &ProtocolMessage) -> bool {
        let Some(transition) = terminal_transition(pane, message) else {
            log::error!("refusing a non-terminal update in the terminal mailbox for {pane}");
            return false;
        };
        self.replace_terminal_with(pane, transition, || {
            zz_protocol::encode_protocol_message(message).map(Arc::from)
        })
    }

    fn replace_terminal_viewport(
        &self,
        pane: PaneId,
        viewport: &TerminalViewport,
        frames: &TerminalFrames,
    ) -> bool {
        self.replace_terminal_with(
            pane,
            TerminalTransition {
                base: None,
                current: viewport_generation(viewport),
            },
            || frames.full(pane, viewport),
        )
    }

    fn replace_terminal_with(
        &self,
        pane: PaneId,
        transition: TerminalTransition,
        encode: impl FnOnce() -> Result<Arc<[u8]>, ProtocolError>,
    ) -> bool {
        {
            let state = self.state.lock();
            if state.closed {
                return false;
            }
            if terminal_update_redundant(&state, pane, transition, false) {
                return false;
            }
        }
        let Ok(encoded) = encode() else {
            log::error!("failed to encode coalesced terminal update for {pane}");
            return false;
        };
        let mut state = self.state.lock();
        if state.closed {
            return false;
        }
        if terminal_update_redundant(&state, pane, transition, false) {
            return false;
        }
        clear_preview_refresh(&mut state, pane);
        let mut replaced_len = state
            .terminals
            .get(&pane)
            .map_or(0, |pending| pending.encoded.len());
        if replaced_len == 0 && state.terminals.len() >= MAX_PENDING_TERMINALS
            || !reserve_outbound_bytes(&mut state, encoded.len(), replaced_len)
        {
            shed_preview_terminals(&mut state);
            replaced_len = state
                .terminals
                .get(&pane)
                .map_or(0, |pending| pending.encoded.len());
        }
        if replaced_len == 0 && state.terminals.len() >= MAX_PENDING_TERMINALS
            || !reserve_outbound_bytes(&mut state, encoded.len(), replaced_len)
        {
            close_outbound(&mut state);
            self.notify_all();
            return false;
        }
        state.queued_bytes = state
            .queued_bytes
            .saturating_sub(replaced_len)
            .saturating_add(encoded.len());
        let idle = state.terminals.is_empty();
        let replaced = state.terminals.insert(
            pane,
            PendingTerminal {
                encoded,
                current: transition.current,
                preview: false,
                full: transition.base.is_none(),
            },
        );
        if let Some(replaced) = replaced {
            discard_outbound_frame(&mut state, replaced.encoded);
        } else {
            state.terminal_order.push_back(pane);
        }
        drop(state);
        if idle {
            self.notify_one();
        }
        true
    }

    fn replace_command_output(&self, message: &ProtocolMessage) -> bool {
        let output_id = match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::CommandOutput {
                        output_id,
                        viewport: Some(_),
                        ..
                    },
                ..
            }) => *output_id,
            _ => return false,
        };
        let Ok(encoded) = self.encode_message(message) else {
            log::error!("failed to encode command-output viewport");
            return false;
        };
        self.replace_encoded_command_output(output_id, encoded)
    }

    fn replace_encoded_command_output(&self, output_id: u64, encoded: Vec<u8>) -> bool {
        let mut state = self.state.lock();
        if state.closed {
            return false;
        }
        if state
            .command_output
            .as_ref()
            .is_some_and(|pending| pending.output_id > output_id)
        {
            recycle_outbound_frame(&mut state, encoded);
            return true;
        }
        let replaced_len = state
            .command_output
            .as_ref()
            .map_or(0, |pending| pending.encoded.len());
        if !reserve_outbound_bytes(&mut state, encoded.len(), replaced_len) {
            shed_preview_terminals(&mut state);
        }
        if !reserve_outbound_bytes(&mut state, encoded.len(), replaced_len) {
            close_outbound(&mut state);
            self.notify_all();
            return false;
        }
        state.queued_bytes = state
            .queued_bytes
            .saturating_sub(replaced_len)
            .saturating_add(encoded.len());
        let replaced = state
            .command_output
            .replace(PendingCommandOutput { output_id, encoded });
        if let Some(replaced) = replaced {
            discard_outbound_frame(&mut state, replaced.encoded);
        }
        drop(state);
        self.notify_one();
        true
    }

    #[cfg(test)]
    fn recv(&self) -> Option<Vec<u8>> {
        let mut state = self.state.lock();
        loop {
            if let Some(frame) = pop_ready_frame(&mut state) {
                state.writer_inflight_bytes = frame.len();
                return Some(frame.into_vec());
            }
            if state.closed {
                return None;
            }
            self.ready.wait(&mut state);
        }
    }

    #[cfg(unix)]
    fn try_recv_batch(&self, frames: &mut Vec<OutboundFrame>, max_bytes: usize) {
        let mut guard = self.state.lock();
        let state = &mut *guard;
        let written = state.written_bytes;
        let now = state.flow.is_some().then(Instant::now);
        let empty = state
            .flow
            .as_mut()
            .zip(now)
            .is_some_and(|(flow, now)| flow.sending(written, now));
        let mut bytes = 0;
        let mut messages = 0;
        let base = frames.len();
        while bytes < max_bytes {
            if let Some(flow) = state.flow.as_mut()
                && !flow.room(written + bytes as u64)
            {
                flow.block();
                break;
            }
            let reliable = !state.reliable.is_empty();
            let Some(frame) = pop_ready_frame(state) else {
                break;
            };
            let frame = frame.materialize();
            bytes += frame.len();
            frames.push(frame);
            if reliable {
                messages = frames.len() - base;
            }
            if let Some((flow, now)) = state.flow.as_mut().zip(now)
                && flow.ping_due(written + bytes as u64, false)
            {
                bytes += push_flow_ping(flow, frames, written + bytes as u64, now);
            }
        }
        if bytes != 0
            && let Some((flow, now)) = state.flow.as_mut().zip(now)
            && flow.ping_due(written + bytes as u64, empty)
        {
            bytes += push_flow_ping(flow, frames, written + bytes as u64, now);
        }
        state.writer_inflight_bytes = bytes;
        state.writer_inflight_messages = messages;
        state.writer_batch_reliable = messages;
    }

    #[cfg(unix)]
    fn record_write(&self, bytes: usize) {
        let mut state = self.state.lock();
        state.writer_inflight_bytes = state.writer_inflight_bytes.saturating_sub(bytes);
        state.written_bytes = state.written_bytes.saturating_add(bytes as u64);
    }

    #[cfg(unix)]
    fn complete_inflight_frame(&self, index: usize) {
        let mut state = self.state.lock();
        if index < state.writer_batch_reliable {
            state.writer_inflight_messages = state.writer_inflight_messages.saturating_sub(1);
        }
    }

    #[cfg(unix)]
    fn recycle_written_batch(&self, frames: &mut Vec<OutboundFrame>) {
        let mut state = self.state.lock();
        for frame in frames.drain(..) {
            recycle_outbound_frame(&mut state, frame);
        }
        state.writer_inflight_bytes = 0;
        state.writer_inflight_messages = 0;
        state.writer_batch_reliable = 0;
    }

    #[cfg(any(windows, test))]
    fn recv_batch(&self, frames: &mut Vec<OutboundFrame>, max_bytes: usize) -> bool {
        let mut state = self.state.lock();
        loop {
            let mut bytes = 0_usize;
            while bytes < max_bytes {
                let Some(frame) = pop_ready_frame(&mut state) else {
                    break;
                };
                bytes = bytes.saturating_add(frame.len());
                frames.push(frame);
            }
            if !frames.is_empty() {
                state.writer_inflight_bytes = bytes;
                return true;
            }
            if state.closed {
                return false;
            }
            self.ready.wait(&mut state);
        }
    }

    #[cfg(any(windows, test))]
    fn finish_batch(&self, frames: &mut Vec<OutboundFrame>) {
        let mut state = self.state.lock();
        for frame in frames.drain(..) {
            state.written_bytes = state.written_bytes.saturating_add(frame.len() as u64);
            recycle_outbound_frame(&mut state, frame);
        }
        state.writer_inflight_bytes = 0;
        state.writer_inflight_messages = 0;
    }

    #[cfg(unix)]
    fn enable_flow(&self) {
        self.state.lock().flow = Some(flow::FlowWindow::new(Instant::now()));
    }

    #[cfg(unix)]
    fn acknowledge(&self, token: u64) {
        if let Some(flow) = self.state.lock().flow.as_mut() {
            flow.acknowledge(token, Instant::now());
        }
    }

    fn close(&self) {
        let mut state = self.state.lock();
        close_outbound(&mut state);
        drop(state);
        self.close_control_feed();
        self.notify_all();
    }

    fn close_control_feed(&self) {
        if let Some(feed) = self.control_feed() {
            feed.close();
        }
    }

    fn close_after_flush(&self) {
        if let Some(feed) = self.control_feed() {
            feed.finish(self);
        }
        let mut state = self.state.lock();
        state.closed = true;
        #[cfg(unix)]
        drop(state.quiet_socket.take());
        #[cfg(unix)]
        drop(state.direct_socket.take());
        if matches!(
            state.ctrl_collecting,
            ControlCollection::Quiet | ControlCollection::Attach
        ) {
            self.flush_control_batch_locked(state, false, false);
            return;
        }
        drop(state);
        self.notify_all();
    }

    #[cfg(any(windows, test))]
    fn buffered() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(OutboundState {
                buffered: true,
                ..OutboundState::default()
            }),
            ready: Condvar::new(),
            #[cfg(unix)]
            loop_waker: Mutex::new(None),
            terminals_frozen: AtomicBool::new(false),
            control: std::sync::OnceLock::new(),
        })
    }

    #[cfg(windows)]
    fn stop_buffering(&self) {
        self.state.lock().buffered = false;
    }

    #[cfg(any(windows, test))]
    fn drain_reliable_into(&self, output: &mut Vec<u8>) {
        let mut state = self.state.lock();
        while let Some(frame) = state.reliable.pop_front() {
            state.queued_bytes = state.queued_bytes.saturating_sub(frame.len());
            let frame = frame.materialize();
            output.extend_from_slice(&frame);
            recycle_outbound_frame(&mut state, frame);
        }
    }

    fn mark_writer_finished(&self) {
        let mut state = self.state.lock();
        if std::thread::panicking() {
            close_outbound(&mut state);
        }
        #[cfg(unix)]
        drop(state.quiet_socket.take());
        #[cfg(unix)]
        drop(state.direct_socket.take());
        state.discarded_bytes = state
            .discarded_bytes
            .saturating_add(state.writer_inflight_bytes as u64);
        state.writer_inflight_bytes = 0;
        state.writer_inflight_messages = 0;
        state.writer_finished = true;
        drop(state);
        self.notify_all();
    }

    fn wait_writer_finished(&self, deadline: Instant) -> bool {
        let mut state = self.state.lock();
        while !state.writer_finished {
            if self.ready.wait_until(&mut state, deadline).timed_out() {
                return state.writer_finished;
            }
        }
        true
    }

    fn queued_reliable(&self) -> Option<(usize, usize)> {
        let state = self.state.lock();
        (!state.closed).then_some((
            state
                .queued_bytes
                .saturating_add(state.writer_inflight_bytes),
            state
                .reliable
                .len()
                .saturating_add(state.writer_inflight_messages),
        ))
    }

    fn is_open(&self) -> bool {
        !self.state.lock().closed
    }

    fn close_too_far_behind(&self) {
        let mut state = self.state.lock();
        close_outbound_too_far_behind(&mut state);
        drop(state);
        self.close_control_feed();
        self.notify_all();
    }

    fn take_preview_refreshes(&self) -> Vec<PaneId> {
        take_preview_refreshes(&mut self.state.lock())
    }

    #[cfg(test)]
    fn cancel_terminal(&self, pane: PaneId) {
        let mut state = self.state.lock();
        remove_pending_terminal(&mut state, pane);
        clear_preview_refresh(&mut state, pane);
        state.delivered_terminals.remove(&pane);
        state.delivered_images.remove(&pane);
        state.delivered_pasted_images.remove(&pane);
    }

    fn suspend_terminal(&self, pane: PaneId) {
        let mut state = self.state.lock();
        remove_pending_terminal(&mut state, pane);
        clear_preview_refresh(&mut state, pane);
        state.delivered_terminals.remove(&pane);
    }

    fn reset_kitty_images(&self) {
        self.state.lock().delivered_images.clear();
    }

    fn reset_pasted_images(&self) {
        self.state.lock().delivered_pasted_images.clear();
    }

    fn log_diagnostic_snapshot(&self, client: ClientId, reason: &str) {
        let state = self.state.lock();
        log::info!(
            target: "zz_daemon::diagnostics::outbound",
            "snapshot reason={reason} client={client} reliable_messages={} command_output_bytes={} agent_panes={} agent_frames={} terminal_messages={} delivered_terminals={} delivered_image_panes={} terminal_order={} preview_refreshes={} recycled_frames={} recycled_capacity={} queued_bytes={} closed={}",
            state.reliable.len(),
            state
                .command_output
                .as_ref()
                .map_or(0, |pending| pending.encoded.len()),
            state.agent.len(),
            state.agent.values().map(|queued| queued.frames.len()).sum::<usize>(),
            state.terminals.len(),
            state.delivered_terminals.len(),
            state.delivered_images.len(),
            state.terminal_order.len(),
            state.preview_refreshes.len(),
            state.recycled_frames.len(),
            state.recycled_capacity,
            state.queued_bytes,
            state.closed,
        );
        log::trace!(
            target: "zz_daemon::diagnostics::outbound",
            "snapshot reason={reason} client={client} reliable_frame_lengths={:?} command_output_capacity={:?} terminals={:#?} delivered_terminals={:#?} terminal_order={:#?} preview_refresh_order={:#?} recycled_frame_capacities={:?}",
            state.reliable.iter().map(OutboundFrame::len).collect::<Vec<_>>(),
            state
                .command_output
                .as_ref()
                .map(|pending| pending.encoded.capacity()),
            state.terminals.iter().map(|(pane, pending)| (*pane, pending.encoded.len(), pending.current)).collect::<Vec<_>>(),
            state.delivered_terminals,
            state.terminal_order,
            state.preview_refresh_order,
            state.recycled_frames.iter().map(Vec::capacity).collect::<Vec<_>>(),
        );
    }
}

fn pop_ready_frame(state: &mut OutboundState) -> Option<OutboundFrame> {
    let partial = state
        .reliable
        .front()
        .is_some_and(OutboundFrame::is_partial);
    if !partial && (state.attach_batch || state.ctrl_collecting != ControlCollection::None) {
        return None;
    }
    if let Some(frame) = state.reliable.pop_front() {
        state.queued_bytes = state.queued_bytes.saturating_sub(frame.len());
        return Some(frame);
    }
    if let Some(pending) = state.command_output.take() {
        state.queued_bytes = state.queued_bytes.saturating_sub(pending.encoded.len());
        return Some(pending.encoded.into());
    }
    while let Some(pane) = state.agent_order.pop_front() {
        let Some(queued) = state.agent.get_mut(&pane) else {
            continue;
        };
        let Some((_, frame)) = queued.frames.pop_front() else {
            state.agent.remove(&pane);
            continue;
        };
        queued.bytes = queued.bytes.saturating_sub(frame.len());
        if queued.frames.is_empty() {
            state.agent.remove(&pane);
        } else {
            state.agent_order.push_back(pane);
        }
        state.queued_bytes = state.queued_bytes.saturating_sub(frame.len());
        return Some(frame);
    }
    if state.terminals_held {
        return None;
    }
    while let Some(pane) = state.terminal_order.pop_front() {
        if let Some(pending) = state.terminals.remove(&pane) {
            state.queued_bytes = state.queued_bytes.saturating_sub(pending.encoded.len());
            state.delivered_terminals.insert(pane, pending.current);
            return Some(pending.encoded.into());
        }
    }
    None
}

#[cfg(unix)]
fn push_flow_ping(
    flow: &mut flow::FlowWindow,
    frames: &mut Vec<OutboundFrame>,
    sent: u64,
    now: Instant,
) -> usize {
    let rtt_us = flow.rtt_us();
    let mut encoded = Vec::new();
    let mut token = sent;
    loop {
        encoded.clear();
        encode_protocol_message_into(&ProtocolMessage::Ping { token, rtt_us }, &mut encoded)
            .expect("a ping always encodes");
        let next = sent + encoded.len() as u64;
        if next == token {
            break;
        }
        token = next;
    }
    flow.pinged(token, now);
    let len = encoded.len();
    frames.push(OutboundFrame::Owned(encoded));
    len
}

fn take_recycled_frame(state: &mut OutboundState) -> Vec<u8> {
    let Some(frame) = state.recycled_frames.pop() else {
        return Vec::new();
    };
    state.recycled_capacity = state.recycled_capacity.saturating_sub(frame.capacity());
    frame
}

fn recycle_outbound_frame(state: &mut OutboundState, frame: impl Into<OutboundFrame>) {
    if state.closed || state.recycled_frames.len() >= MAX_RECYCLED_FRAME_BUFFERS {
        return;
    }
    let mut frame = match frame.into() {
        OutboundFrame::Owned(frame) => frame,
        OutboundFrame::Shared(_) => return,
        OutboundFrame::Grouped { encoded, frames } => {
            recycle_outbound_frame(state, encoded);
            for frame in frames {
                recycle_outbound_frame(state, frame);
            }
            return;
        }
        OutboundFrame::DeferredGrouped { frames, .. } => {
            for frame in frames {
                recycle_outbound_frame(state, frame);
            }
            return;
        }
        #[cfg(unix)]
        OutboundFrame::Partial { frame, .. } => {
            recycle_outbound_frame(state, *frame);
            return;
        }
    };
    let capacity = frame.capacity();
    if capacity == 0
        || state
            .recycled_capacity
            .checked_add(capacity)
            .is_none_or(|total| total > MAX_RECYCLED_FRAME_CAPACITY)
    {
        return;
    }
    frame.clear();
    state.recycled_capacity += capacity;
    state.recycled_frames.push(frame);
}

fn discard_outbound_frame(state: &mut OutboundState, frame: impl Into<OutboundFrame>) {
    let frame = frame.into();
    state.discarded_bytes = state.discarded_bytes.saturating_add(frame.len() as u64);
    recycle_outbound_frame(state, frame);
}

fn clear_pending_agent(state: &mut OutboundState, pane: PaneId) {
    if let Some(queued) = state.agent.remove(&pane) {
        state.queued_bytes = state.queued_bytes.saturating_sub(queued.bytes);
        for (_, frame) in queued.frames {
            discard_outbound_frame(state, frame);
        }
    }
    state.agent_order.retain(|queued| *queued != pane);
}

fn remove_pending_terminal(state: &mut OutboundState, pane: PaneId) {
    if let Some(pending) = state.terminals.remove(&pane) {
        state.queued_bytes = state.queued_bytes.saturating_sub(pending.encoded.len());
        discard_outbound_frame(state, pending.encoded);
    }
    state.terminal_order.retain(|queued| *queued != pane);
}

fn mark_preview_refresh(state: &mut OutboundState, pane: PaneId) {
    if state.preview_refreshes.len() < MAX_PENDING_TERMINALS && state.preview_refreshes.insert(pane)
    {
        state.preview_refresh_order.push_back(pane);
    }
}

fn clear_preview_refresh(state: &mut OutboundState, pane: PaneId) {
    if state.preview_refreshes.remove(&pane) {
        state.preview_refresh_order.retain(|queued| *queued != pane);
    }
}

fn take_preview_refreshes(state: &mut OutboundState) -> Vec<PaneId> {
    let OutboundState {
        preview_refreshes,
        preview_refresh_order,
        ..
    } = state;
    preview_refresh_order
        .drain(..)
        .filter(|pane| preview_refreshes.remove(pane))
        .collect()
}

fn shed_preview_terminals(state: &mut OutboundState) {
    let panes = state
        .terminals
        .iter()
        .filter_map(|(pane, pending)| pending.preview.then_some(*pane))
        .collect::<Vec<_>>();
    for pane in panes {
        mark_preview_refresh(state, pane);
        remove_pending_terminal(state, pane);
    }
}

fn reserve_outbound_bytes(state: &mut OutboundState, incoming: usize, replaced: usize) -> bool {
    state
        .queued_bytes
        .saturating_add(state.writer_inflight_bytes)
        .saturating_sub(replaced)
        .checked_add(incoming)
        .is_some_and(|total| total <= MAX_OUTBOUND_BYTES)
}

#[cfg(unix)]
fn try_write_quiet_group(state: &mut OutboundState) -> bool {
    if state.closed
        || state.buffered
        || state.writer_finished
        || state.writer_inflight_bytes != 0
        || state.reliable.len() != 1
        || state.command_output.is_some()
        || !state.agent.is_empty()
        || !state.terminals.is_empty()
        || !state.preview_refreshes.is_empty()
    {
        return false;
    }
    let Some(socket) = &state.quiet_socket else {
        return false;
    };
    let Some(frame @ OutboundFrame::Grouped { .. }) = state.reliable.front() else {
        return false;
    };
    if state.queued_bytes != frame.len() {
        return false;
    }
    let written = match send_nonblocking(socket, frame) {
        Ok(0) => return false,
        Ok(written) => written,
        Err(error) if error == rustix::io::Errno::INTR || error == rustix::io::Errno::AGAIN => {
            return false;
        }
        Err(_) => {
            close_outbound(state);
            return false;
        }
    };
    let frame = state.reliable.pop_front().expect("sole quiet group");
    state.queued_bytes = state.queued_bytes.saturating_sub(written);
    state.written_bytes = state.written_bytes.saturating_add(written as u64);
    if written == frame.len() {
        recycle_outbound_frame(state, frame);
        true
    } else {
        state.reliable.push_front(OutboundFrame::Partial {
            frame: Box::new(frame),
            offset: written,
        });
        false
    }
}

#[cfg(unix)]
fn send_nonblocking(socket: &std::os::fd::OwnedFd, bytes: &[u8]) -> rustix::io::Result<usize> {
    let flags = rustix::net::SendFlags::DONTWAIT;
    #[cfg(not(any(target_vendor = "apple", target_os = "redox", target_os = "vita")))]
    let flags = flags | rustix::net::SendFlags::NOSIGNAL;
    rustix::net::send(socket, bytes, flags)
}

#[cfg(unix)]
fn write_direct_terminal(state: &mut OutboundState, frame: &[u8]) -> Option<usize> {
    if state.queued_bytes != 0
        || !state.reliable.is_empty()
        || !state.terminals.is_empty()
        || state.writer_inflight_bytes != 0
        || state.writer_finished
        || state.attach_batch
        || state.terminals_held
        || state.attach_settling.is_some()
        || state.ctrl_collecting != ControlCollection::None
    {
        return None;
    }
    let socket = state.direct_socket.as_ref()?;
    match send_nonblocking(socket, frame) {
        Ok(0) => None,
        Ok(written) => {
            state.written_bytes = state.written_bytes.saturating_add(written as u64);
            Some(written)
        }
        Err(error) if error == rustix::io::Errno::INTR || error == rustix::io::Errno::AGAIN => None,
        Err(_) => {
            state.direct_socket = None;
            None
        }
    }
}

#[cfg(all(test, unix))]
mod direct_write_tests;

#[cfg(all(test, unix))]
mod typed_burst_tests;

fn close_outbound(state: &mut OutboundState) {
    #[cfg(unix)]
    if let Some(socket) = state.quiet_socket.take() {
        let _ = rustix::net::shutdown(&socket, rustix::net::Shutdown::Both);
    }
    #[cfg(unix)]
    drop(state.direct_socket.take());
    state.discarded_bytes = state
        .discarded_bytes
        .saturating_add(state.queued_bytes as u64);
    state.closed = true;
    state.attach_batch = false;
    state.terminals_held = false;
    state.ctrl_collecting = ControlCollection::None;
    state.reliable.clear();
    state.command_output = None;
    state.agent.clear();
    state.agent_order.clear();
    state.terminals.clear();
    state.delivered_terminals.clear();
    state.delivered_images.clear();
    state.delivered_pasted_images.clear();
    state.terminal_order.clear();
    state.preview_refreshes.clear();
    state.preview_refresh_order.clear();
    state.recycled_frames.clear();
    state.recycled_capacity = 0;
    state.queued_bytes = 0;
}

fn close_outbound_too_far_behind(state: &mut OutboundState) {
    let message = ProtocolMessage::Event(Event {
        sequence: 0,
        payload: EventPayload::ControlExit {
            reason: "too far behind".to_owned(),
        },
    });
    let mut encoded = Vec::new();
    let encoded = encode_protocol_message_into(&message, &mut encoded)
        .is_ok()
        .then_some(encoded);
    let partial = state
        .reliable
        .front()
        .is_some_and(OutboundFrame::is_partial)
        .then(|| state.reliable.pop_front())
        .flatten();
    let keep_partial = partial.as_ref().is_none_or(|partial| {
        encoded.as_ref().is_some_and(|encoded| {
            partial
                .len()
                .checked_add(encoded.len())
                .is_some_and(|bytes| bytes <= MAX_OUTBOUND_BYTES)
                && MAX_RELIABLE_MESSAGES >= 2
        })
    });
    if keep_partial && let Some(partial) = &partial {
        state.queued_bytes = state.queued_bytes.saturating_sub(partial.len());
    }
    #[cfg(unix)]
    let socket = state.quiet_socket.take();
    #[cfg(unix)]
    if (!keep_partial || encoded.is_none())
        && let Some(socket) = &socket
    {
        let _ = rustix::net::shutdown(socket, rustix::net::Shutdown::Both);
    }
    close_outbound(state);
    if !keep_partial {
        return;
    }
    #[cfg(unix)]
    if socket.is_some() {
        state.attach_batch = false;
        state.terminals_held = false;
    }
    if let Some(partial) = partial {
        state.queued_bytes = partial.len();
        state.reliable.push_back(partial);
        state.attach_batch = false;
        state.terminals_held = false;
    }
    if let Some(encoded) = encoded {
        state.queued_bytes += encoded.len();
        state.reliable.push_back(encoded.into());
    }
}

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

#[derive(Clone, PartialEq)]
struct PendingHookEvent {
    name: &'static str,
    context: ExecutionContext,
    variables: BTreeMap<String, String>,
    exclude_client: Option<ClientId>,
    control_notified: bool,
}

const HOOK_CONTEXT_FORMAT: &str = "hook";
const HOOK_CLIENT_CONTEXT_FORMAT: &str = "hook_client";
const HOOK_PANE_CONTEXT_FORMAT: &str = "hook_pane";
const HOOK_SESSION_CONTEXT_FORMAT: &str = "hook_session";
const HOOK_SESSION_NAME_CONTEXT_FORMAT: &str = "hook_session_name";
const HOOK_WINDOW_CONTEXT_FORMAT: &str = "hook_window";
const HOOK_WINDOW_NAME_CONTEXT_FORMAT: &str = "hook_window_name";
#[cfg(test)]
const NOTIFY_ADD_CONTEXT_FORMATS: [&str; 7] = [
    HOOK_CONTEXT_FORMAT,
    HOOK_CLIENT_CONTEXT_FORMAT,
    HOOK_PANE_CONTEXT_FORMAT,
    HOOK_SESSION_CONTEXT_FORMAT,
    HOOK_SESSION_NAME_CONTEXT_FORMAT,
    HOOK_WINDOW_CONTEXT_FORMAT,
    HOOK_WINDOW_NAME_CONTEXT_FORMAT,
];

#[cfg(test)]
const PRODUCED_NON_AFTER_PINNED_HOOKS: &[&str] = &[
    "alert-activity",
    "alert-bell",
    "alert-silence",
    "client-active",
    "client-attached",
    "client-closed",
    "client-created",
    "client-dark-theme",
    "client-detached",
    "client-focus-in",
    "client-focus-out",
    "client-light-theme",
    "client-resized",
    "client-session-changed",
    "command-error",
    "marked-pane-changed",
    "pane-activity",
    "pane-bell",
    "pane-command-finished",
    "pane-command-started",
    "pane-created",
    "pane-died",
    "pane-exited",
    "pane-focus-in",
    "pane-focus-out",
    "pane-mode-changed",
    "pane-mode-entered",
    "pane-mode-exited",
    "pane-moved",
    "pane-prompt-closed",
    "pane-prompt-opened",
    "pane-resized",
    "pane-set-clipboard",
    "pane-shell-prompt",
    "pane-title-changed",
    "session-closed",
    "session-created",
    "session-renamed",
    "session-window-changed",
    "window-closed",
    "window-created",
    "window-layout-changed",
    "window-linked",
    "window-pane-changed",
    "window-renamed",
    "window-resized",
    "window-unlinked",
    "window-unzoomed",
    "window-zoomed",
];

mod hook_events;
#[cfg(all(test, unix))]
mod hook_events_tests;
#[cfg(test)]
use hook_events::mux_hook_events;

#[derive(Clone)]
struct HookSessionState {
    name: String,
    active_window: WindowId,
}

#[derive(Clone)]
struct HookWindowState {
    session: SessionId,
    index: u32,
    name: String,
    active_pane: PaneId,
    zoomed_pane: Option<PaneId>,
    layout: CellLayout,
    extent: (u16, u16),
}

#[derive(Clone)]
struct HookPaneState {
    session: SessionId,
    window: WindowId,
    title: String,
}

#[derive(Default)]
struct MuxHookSnapshot {
    sessions: BTreeMap<SessionId, HookSessionState>,
    windows: BTreeMap<WindowId, HookWindowState>,
    panes: BTreeMap<PaneId, HookPaneState>,
    links: BTreeSet<(SessionId, WindowId)>,
}

impl PendingHookEvent {
    fn session(
        name: &'static str,
        session: SessionId,
        state: &HookSessionState,
        snapshot: &MuxHookSnapshot,
    ) -> Self {
        hook_events::session_event(name, session, &state.name, snapshot)
    }

    #[cfg(test)]
    fn window(name: &'static str, window: WindowId, snapshot: &MuxHookSnapshot) -> Self {
        hook_events::window_event(name, window, snapshot)
    }

    fn winlink(
        name: &'static str,
        session: SessionId,
        session_state: &HookSessionState,
        window: WindowId,
        window_state: &HookWindowState,
        snapshot: &MuxHookSnapshot,
    ) -> Self {
        hook_events::winlink_event(
            name,
            session,
            &session_state.name,
            window,
            &window_state.name,
            snapshot,
        )
    }

    #[cfg(test)]
    fn pane(
        name: &'static str,
        pane: PaneId,
        state: &HookPaneState,
        snapshot: &MuxHookSnapshot,
    ) -> Self {
        hook_events::pane_event(name, pane, state.session, state.window, snapshot)
    }

    fn pane_named(
        name: &'static str,
        pane: PaneId,
        session: SessionId,
        window: WindowId,
        window_name: String,
    ) -> Self {
        Self {
            name,
            context: ExecutionContext::new(Some(session), Some(window), Some(pane)),
            exclude_client: None,
            control_notified: false,
            variables: BTreeMap::from([
                (HOOK_CONTEXT_FORMAT.to_owned(), name.to_owned()),
                (HOOK_WINDOW_CONTEXT_FORMAT.to_owned(), window.to_string()),
                (HOOK_WINDOW_NAME_CONTEXT_FORMAT.to_owned(), window_name),
                (HOOK_PANE_CONTEXT_FORMAT.to_owned(), pane.to_string()),
            ]),
        }
    }

    fn client(
        name: &'static str,
        context: ExecutionContext,
        client: ClientId,
        client_name: Option<&str>,
    ) -> Self {
        let variables = BTreeMap::from([
            (HOOK_CONTEXT_FORMAT.to_owned(), name.to_owned()),
            (
                HOOK_CLIENT_CONTEXT_FORMAT.to_owned(),
                client_name.map_or_else(|| format!("device-{}", client.0), str::to_owned),
            ),
        ]);
        Self {
            name,
            context,
            variables,
            exclude_client: (name == "client-detached").then_some(client),
            control_notified: false,
        }
    }

    fn paste_buffer(name: &'static str, buffer: String) -> Self {
        Self {
            name,
            context: ExecutionContext::new(None, None, None),
            exclude_client: None,
            control_notified: false,
            variables: BTreeMap::from([
                (HOOK_CONTEXT_FORMAT.to_owned(), name.to_owned()),
                ("hook_paste_buffer".to_owned(), buffer),
            ]),
        }
    }
}

impl MuxHookSnapshot {
    fn capture(engine: &MuxEngine) -> Self {
        let mut snapshot = Self::default();
        for (session, state) in &engine.state.sessions {
            snapshot.add_session(*session, &state.name, state.active_window, &state.windows);
        }
        for (window, state) in &engine.state.windows {
            snapshot.add_window(engine, *window, state);
        }
        snapshot
    }

    fn add_session(
        &mut self,
        session: SessionId,
        name: &str,
        active_window: WindowId,
        windows: &[WindowId],
    ) {
        self.sessions.insert(
            session,
            HookSessionState {
                name: name.to_owned(),
                active_window,
            },
        );
        self.links
            .extend(windows.iter().map(|window| (session, *window)));
    }

    fn add_window(&mut self, engine: &MuxEngine, window: WindowId, state: &zz_mux::Window) {
        self.windows.insert(
            window,
            HookWindowState {
                session: state.session,
                index: state.index,
                name: state.name.clone(),
                active_pane: state.active_pane,
                zoomed_pane: state.zoomed_pane,
                layout: state.layout.clone(),
                extent: (
                    engine
                        .window_extent(window, zz_protocol::Axis::Horizontal)
                        .unwrap_or_default(),
                    engine
                        .window_extent(window, zz_protocol::Axis::Vertical)
                        .unwrap_or_default(),
                ),
            },
        );
        self.panes
            .extend(state.panes.iter().map(|(pane, pane_state)| {
                (
                    *pane,
                    HookPaneState {
                        session: state.session,
                        window,
                        title: pane_state.title.clone(),
                    },
                )
            }));
    }

    #[cfg(test)]
    fn session_context(&self, session: SessionId) -> ExecutionContext {
        hook_events::session_context(self, session)
    }
}

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

#[derive(Debug)]
struct PendingPastedImage {
    token: u64,
    format: PastedImageFormat,
    bytes: Arc<[u8]>,
}

#[derive(Debug)]
struct StoredPastedImage {
    token: u64,
    format: PastedImageFormat,
    bytes: Arc<[u8]>,
    frames: Option<Arc<[Vec<u8>]>>,
}

#[derive(Default, Debug)]
struct PanePastedImages {
    pending: VecDeque<PendingPastedImage>,
    images: BTreeMap<u32, StoredPastedImage>,
    order: VecDeque<(u32, u64)>,
    pending_bytes: usize,
    stored_bytes: usize,
}

struct PastedImageAdmission {
    evicted_numbers: Vec<u32>,
    retained: bool,
}

impl PanePastedImages {
    fn push_pending(&mut self, pending: PendingPastedImage) -> PastedImageAdmission {
        let token = pending.token;
        self.pending_bytes = self.pending_bytes.saturating_add(pending.bytes.len());
        self.pending.push_back(pending);
        let evicted_numbers = self.enforce_limits();
        PastedImageAdmission {
            evicted_numbers,
            retained: self.pending.iter().any(|pending| pending.token == token),
        }
    }

    fn bind(&mut self, token: u64, number: u32) -> Option<Vec<u32>> {
        let position = self
            .pending
            .iter()
            .position(|pending| pending.token == token)?;
        let pending = self.pending.remove(position)?;
        self.pending_bytes = self.pending_bytes.saturating_sub(pending.bytes.len());
        if let Some(replaced) = self.images.remove(&number) {
            self.stored_bytes = self.stored_bytes.saturating_sub(replaced.bytes.len());
            self.order.retain(|(stored_number, stored_token)| {
                *stored_number != number || *stored_token != replaced.token
            });
        }
        self.stored_bytes = self.stored_bytes.saturating_add(pending.bytes.len());
        self.order.push_back((number, token));
        self.images.insert(
            number,
            StoredPastedImage {
                token,
                format: pending.format,
                bytes: pending.bytes,
                frames: None,
            },
        );
        Some(self.enforce_limits())
    }

    fn enforce_limits(&mut self) -> Vec<u32> {
        let mut evicted = Vec::new();
        while self.images.len().saturating_add(self.pending.len()) > MAX_PASTED_IMAGES_PER_PANE
            || self.stored_bytes.saturating_add(self.pending_bytes)
                > MAX_PASTED_IMAGE_BYTES_PER_PANE
        {
            while self.order.front().is_some_and(|(number, token)| {
                self.images
                    .get(number)
                    .is_none_or(|image| image.token != *token)
            }) {
                self.order.pop_front();
            }
            if let Some((evicted_number, evicted_token)) = self.order.pop_front() {
                if let Some(image) = self.images.get(&evicted_number)
                    && image.token == evicted_token
                {
                    let image = self
                        .images
                        .remove(&evicted_number)
                        .expect("the pasted image was present");
                    self.stored_bytes = self.stored_bytes.saturating_sub(image.bytes.len());
                    evicted.push(evicted_number);
                }
            } else if let Some(pending) = self.pending.pop_back() {
                self.pending_bytes = self.pending_bytes.saturating_sub(pending.bytes.len());
            } else {
                break;
            }
        }
        evicted
    }

    fn expire(&mut self, token: u64) {
        if let Some(position) = self
            .pending
            .iter()
            .position(|pending| pending.token == token)
            && let Some(pending) = self.pending.remove(position)
        {
            self.pending_bytes = self.pending_bytes.saturating_sub(pending.bytes.len());
        }
    }
}

struct ClientRegistrationGuard {
    shared: Arc<Shared>,
    client: ClientId,
    armed: bool,
}

struct ClientWriterRegistrationGuard {
    shared: Arc<Shared>,
    client: ClientId,
    armed: bool,
}

struct ResponseAdmissionGuard {
    shared: Arc<Shared>,
    armed: bool,
}

#[derive(Default)]
struct ResponseAdmissionState {
    active: usize,
    frozen: bool,
}

#[derive(Default)]
struct ShutdownBlockerState {
    active: usize,
    detached: usize,
    closed: bool,
}

#[cfg(test)]
struct ResponseAdmissionHook {
    reached: crossbeam_channel::Sender<()>,
    release: crossbeam_channel::Receiver<()>,
}

impl ClientRegistrationGuard {
    fn new(shared: &Arc<Shared>, client: ClientId) -> Self {
        Self {
            shared: Arc::clone(shared),
            client,
            armed: true,
        }
    }

    fn unregister(&mut self) {
        if std::mem::take(&mut self.armed) {
            #[cfg(unix)]
            if self.shared.loop_active.load(Ordering::Acquire) {
                self.shared.lifecycle.release(self.client, false);
                self.shared.accept_wake.wake();
                return;
            }
            self.shared.unregister(self.client);
        }
    }
}

impl Drop for ClientRegistrationGuard {
    fn drop(&mut self) {
        self.unregister();
    }
}

impl ClientWriterRegistrationGuard {
    fn new(shared: &Arc<Shared>, client: ClientId, outbound: Arc<OutboundMailbox>) -> Self {
        shared.client_writers.lock().insert(client, outbound);
        Self {
            shared: Arc::clone(shared),
            client,
            armed: true,
        }
    }

    fn unregister(&mut self) {
        if std::mem::take(&mut self.armed) {
            self.shared.client_writers.lock().remove(&self.client);
        }
    }
}

impl Drop for ClientWriterRegistrationGuard {
    fn drop(&mut self) {
        self.unregister();
    }
}

impl ResponseAdmissionGuard {
    fn new(shared: &Arc<Shared>) -> Option<Self> {
        let mut state = shared.response_admissions.lock();
        if state.frozen {
            return None;
        }
        state.active = state
            .active
            .checked_add(1)
            .expect("response admission overflow");
        drop(state);
        Some(Self {
            shared: Arc::clone(shared),
            armed: true,
        })
    }

    fn finish(&mut self) {
        if !std::mem::take(&mut self.armed) {
            return;
        }
        let mut state = self.shared.response_admissions.lock();
        state.active = state
            .active
            .checked_sub(1)
            .expect("response admission underflow");
        let notify = state.active == 0 && state.frozen;
        drop(state);
        if notify {
            self.shared.response_admissions_changed.notify_all();
            self.shared.accept_wake.wake();
        }
    }
}

impl Drop for ResponseAdmissionGuard {
    fn drop(&mut self) {
        self.finish();
    }
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
enum SourceInvocationAccounting {
    #[default]
    Runtime,
    Startup {
        used: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceFileRequest {
    path: RawText,
    quiet: bool,
    parse_only: bool,
    verbose: bool,
    context: ExecutionContext,
    stdin: Option<SourceStream>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SourceFileLoadOptions {
    parse_only: bool,
    verbose: bool,
    suppress_verbose: bool,
    control_target: Option<(ClientId, u8)>,
    replay_client: Option<ClientId>,
    suppress_replay_output: bool,
    command_client: bool,
}

struct PendingConfigFile {
    path: PathBuf,
    context: ExecutionContext,
    options: SourceFileLoadOptions,
    stdin: Option<RawText>,
}

#[derive(Clone, Copy, Debug)]
enum ConfigInput<'a> {
    Text(&'a str),
    FileBytes(&'a [u8]),
    BufferBytes(&'a [u8]),
}

impl<'a> ConfigInput<'a> {
    fn startup(bytes: &'a [u8]) -> Self {
        str::from_utf8(bytes).map_or(Self::FileBytes(bytes), Self::Text)
    }

    fn sourced(bytes: &'a [u8]) -> Self {
        str::from_utf8(bytes).map_or(Self::BufferBytes(bytes), Self::Text)
    }

    fn is_empty(self) -> bool {
        match self {
            Self::Text(text) => text.is_empty(),
            Self::FileBytes(bytes) | Self::BufferBytes(bytes) => bytes.is_empty(),
        }
    }
}

fn non_utf8_config_error() -> std::io::Error {
    std::io::Error::new(ErrorKind::InvalidData, "stream did not contain valid UTF-8")
}

struct PreparedConfig {
    construction: Result<Vec<PreparedConfigCommand>, PreparedConfigFailure>,
    empty_command_item: bool,
}

struct PreparedConfigCommand {
    original: CommandInvocation,
    routed: CommandInvocation,
}

struct PreparedConfigFailure {
    original: CommandInvocation,
    message: String,
}

struct DeferredControlConfigWarning {
    client: ClientId,
    text: String,
}

impl SourceInvocationAccounting {
    fn is_startup(&self) -> bool {
        matches!(self, Self::Startup { .. })
    }

    fn refuses(&mut self, depth: usize) -> bool {
        match self {
            Self::Runtime => depth + 2 > MAX_SOURCE_INVOCATIONS,
            Self::Startup { used } if *used >= MAX_SOURCE_INVOCATIONS => true,
            Self::Startup { used } => {
                *used += 1;
                false
            }
        }
    }
}

fn config_command_error(command: &CommandInvocation, message: &str) -> String {
    command.source.as_ref().map_or_else(
        || message.to_owned(),
        |source| format!("{}:{}: {message}", source.source, source.line),
    )
}

enum ConfigReplayIssue {
    SourceErrors(usize),
    CommandError {
        source: Option<SourceSpan>,
        message: String,
    },
}

enum DeferredConfigReplayIssue {
    Callback(CallbackParseFailure),
    Replay(ConfigReplayIssue),
}

struct ConfigSourceBoundary {
    token: cmdq::ContinuationToken,
    children: VecDeque<(PendingConfigFile, PreparedConfig, Vec<String>)>,
    command: CommandInvocation,
    group: Option<(String, u32)>,
    source_error_group: Option<usize>,
    source_error: Option<DaemonError>,
    source_command_error: bool,
    source_has_file: bool,
    warnings: Vec<DeferredControlConfigWarning>,
}

enum ConfigFrameAction {
    Command(Box<(source_queue::CommandFinish, Result<Execution, DaemonError>)>),
    ReadSource(Box<source_queue::SourceRead>),
    Continue,
    Source(Box<ConfigSourceBoundary>),
    Finish,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplayStdoutWrite {
    Written,
    Dropped,
    Denied,
}

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

struct PanePipe {
    token: u64,
    pid: u32,
    terminal: Arc<Mutex<Arc<TerminalSession>>>,
    tap_output: Option<Arc<shard_sink::PipeFeed>>,
    #[cfg(unix)]
    cancel: Arc<AtomicBool>,
    #[cfg(unix)]
    jobs: pipe_jobs::Client,
    #[cfg(windows)]
    process: Arc<Mutex<Option<Child>>>,
    #[cfg(windows)]
    stop: Arc<AtomicBool>,
    #[cfg(windows)]
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for PanePipe {
    fn drop(&mut self) {
        if let Some(output) = &self.tap_output {
            output.end();
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CopySession {
    pane: PaneId,
    observed: bool,
    scroll_exit: bool,
    kill: bool,
    refresh: bool,
    unseen: bool,
    sourced: bool,
    exiting: bool,
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
struct DisplayPanesSession {
    token: u64,
    source_pane: PaneId,
    source_session: SessionId,
    selectable: bool,
    state: DisplayPanesState,
    template: Option<CommandPromptTemplate>,
    source: Option<SourceSpan>,
    waiter: Option<OverlayWait>,
    mode: Option<PaneId>,
}

fn take_display_panes(inner: &mut ServerState, client: ClientId) -> Option<DisplayPanesSession> {
    inner
        .client_mut(client)
        .and_then(|c| c.display_panes.take())
}

fn dismiss_display_panes(inner: &mut ServerState, client: ClientId) -> bool {
    let Some(overlay) = take_display_panes(inner, client) else {
        return false;
    };
    if let Some(host) = overlay.mode
        && matches!(
            inner.modes.pane_modes.get(&host).and_then(|modes| modes.last()),
            Some(PaneModeRequest::Panes(mode)) if mode.token == overlay.token
        )
    {
        pop_pane_mode(inner, host);
    }
    true
}

fn display_panes_overlay_mode(
    inner: &ServerState,
    client: ClientId,
) -> Option<(PaneId, PanesMode)> {
    let registered = inner.client(client)?;
    if registered.subscriber.is_none()
        || registered.kind != Some(ClientKind::Interactive)
        || registered.exits_on_detach
    {
        return None;
    }
    let window = client_focused_window_for_attachment(inner, client)?;
    let state = inner.engine.state.windows.get(&window)?;
    std::iter::once(state.active_pane)
        .chain(state.pane_order().iter().copied())
        .find_map(|pane| {
            match inner
                .modes
                .pane_modes
                .get(&pane)
                .and_then(|modes| modes.last())
            {
                Some(PaneModeRequest::Panes(mode)) => Some((pane, mode.as_ref().clone())),
                _ => None,
            }
        })
}

fn sync_display_panes_overlays(
    inner: &mut ServerState,
) -> Vec<(ClientId, Option<DisplayPanesState>)> {
    let clients = inner.clients.keys().copied().collect::<Vec<_>>();
    let mut updates = Vec::new();
    for client in clients {
        let wanted = display_panes_overlay_mode(inner, client);
        let current = take_display_panes(inner, client);
        let shown = current.is_some();
        let built = wanted.and_then(|(host, mode)| {
            let session = client_attached_session(inner, client)?;
            let host_window = inner.engine.state.window_for_pane(host)?;
            let source_pane = if host_window == mode.source_window {
                host
            } else {
                inner
                    .engine
                    .state
                    .windows
                    .get(&mode.source_window)?
                    .active_pane
            };
            let facts = borrowed_format_hook_facts(inner);
            let (_, _, state) = build_display_panes_state(
                &inner.engine,
                &inner.config.config_files,
                &facts,
                source_pane,
                mode.duration_ms,
                session,
            )
            .ok()?;
            Some((host, mode, session, source_pane, state))
        });
        let Some((host, mode, session, source_pane, state)) = built else {
            if shown {
                updates.push((client, None));
            }
            continue;
        };
        let current = current.filter(|overlay| {
            overlay.token == mode.token
                && overlay.mode == Some(host)
                && overlay.source_session == session
        });
        if current
            .as_ref()
            .is_none_or(|overlay| overlay.state != state)
        {
            updates.push((client, Some(state.clone())));
        }
        inner
            .client_entry(client)
            .display_panes
            .replace(DisplayPanesSession {
                token: mode.token,
                source_pane,
                source_session: session,
                selectable: !mode.ignore_keys,
                state,
                template: mode.template,
                source: mode.source,
                waiter: current.and_then(|overlay| overlay.waiter),
                mode: Some(host),
            });
    }
    updates
}

fn build_display_panes_state(
    engine: &MuxEngine,
    config_files: &str,
    facts: &dyn crate::status::FormatFactSource,
    source_pane: PaneId,
    duration_ms: u32,
    format_client_session: SessionId,
) -> Result<(SessionId, WindowId, DisplayPanesState), ServerError> {
    let state = &engine.state;
    let window_id = state
        .window_for_pane(source_pane)
        .ok_or_else(|| ServerError::MissingTarget(source_pane.to_string()))?;
    let window = state
        .windows
        .get(&window_id)
        .ok_or_else(|| ServerError::MissingTarget(window_id.to_string()))?;
    let panes = window.pane_order().to_vec();
    if panes.len() > MAX_DISPLAY_PANE_INDICATORS {
        return Err(ServerError::Internal(format!(
            "window {window_id} exceeds the pane indicator limit"
        )));
    }
    let format = engine.display_panes_format_for_window(window_id);
    let indicators = panes
        .into_iter()
        .filter(|pane| window.zoomed_pane.is_none_or(|zoomed| *pane == zoomed))
        .map(|pane| {
            let index = engine
                .pane_index(window_id, pane)
                .expect("window pane has an effective index");
            let select_key = match index {
                0..=9 => b'0' + u8::try_from(index).expect("digit pane index"),
                10..=35 => b'a' + u8::try_from(index - 10).expect("letter pane index"),
                _ => 0,
            };
            let label = if format.is_empty() {
                String::new()
            } else {
                let mut context = engine.format_status_context_for_client(
                    Some(window.session),
                    Some(window_id),
                    Some(pane),
                    format_client_session,
                );
                context.set_format_value("config_files", config_files);
                let mut hooks = DaemonFormatHooks::command(facts).with_option_engine(engine);
                truncate_pane_indicator_label(expand_format_values(&format, &context, &mut hooks))
            };
            PaneIndicator {
                pane,
                index,
                select_key,
                flags: if pane == window.active_pane {
                    PaneIndicator::ACTIVE
                } else {
                    0
                },
                label,
            }
        })
        .collect();
    let colour_context = engine.format_status_context_for_client(
        Some(window.session),
        Some(window_id),
        Some(window.active_pane),
        format_client_session,
    );
    let colour = |name: &str| {
        let value = engine.format_option_value(&colour_context, name)?;
        zz_protocol::parse_tmux_colour(&value)
            .or_else(|| zz_protocol::parse_style(&value).and_then(|style| style.fg))
    };
    Ok((
        window.session,
        window_id,
        DisplayPanesState {
            window: window_id,
            duration_ms,
            indicators,
            colour: colour("display-panes-colour"),
            active_colour: colour("display-panes-active-colour"),
        },
    ))
}

fn bounded_choose_item_text(text: &str) -> String {
    let mut bounded = String::with_capacity(text.len().min(MAX_CHOOSE_ITEM_TEXT_BYTES));
    for character in text.chars() {
        if bounded.len() + character.len_utf8() > MAX_CHOOSE_ITEM_TEXT_BYTES {
            break;
        }
        bounded.push(character);
    }
    bounded
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChooserPromptKind {
    Kill,
    Command,
    Filter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChooserPrompt {
    kind: ChooserPromptKind,
    text: String,
    editor: ModePrompt,
    targets: Vec<ChooseTreeTarget>,
}

impl ChooserPrompt {
    fn new(
        kind: ChooserPromptKind,
        text: String,
        input: &str,
        targets: Vec<ChooseTreeTarget>,
    ) -> Self {
        let editor = ModePrompt::new(text.clone(), input, "");
        Self {
            kind,
            text,
            editor,
            targets,
        }
    }

    const fn history_type(&self) -> CommandPromptType {
        match self.kind {
            ChooserPromptKind::Filter => CommandPromptType::Search,
            ChooserPromptKind::Kill | ChooserPromptKind::Command => CommandPromptType::Command,
        }
    }

    fn input(&self) -> String {
        self.editor.input()
    }

    fn edit(
        &mut self,
        edit: &ChooserPromptEdit,
        history: &[String],
        (vi, separators): (bool, &str),
    ) -> PromptOutcome {
        self.editor.set_key_options(vi, separators);
        match edit {
            ChooserPromptEdit::Append(text) => {
                if self.input().len().saturating_add(text.len()) > MAX_CHOOSE_TREE_QUERY_BYTES {
                    return PromptOutcome::Handled;
                }
                for character in text.chars() {
                    let outcome = self
                        .editor
                        .key_with_history(ModeKey::Char(character), history);
                    if matches!(outcome, PromptOutcome::Done | PromptOutcome::Cancelled) {
                        return outcome;
                    }
                }
                PromptOutcome::Handled
            }
            ChooserPromptEdit::Key(key) => self.editor.key_with_history(*key, history),
        }
    }

    fn remembered(&self) -> Option<(CommandPromptType, String)> {
        let input = self.input();
        (!input.is_empty()).then(|| (self.history_type(), input))
    }

    fn line(&self) -> String {
        format!("{}{}", self.text, self.input())
    }
}

#[derive(Clone, Debug)]
struct ClientChooserRow {
    client: ClientId,
    name: String,
    text: String,
    activity: u64,
    created: u64,
    width: u16,
    height: u16,
    matches: bool,
    pane: Option<PaneId>,
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

fn enter_copy_session(
    inner: &mut ServerState,
    client: ClientId,
    pane: PaneId,
) -> Result<(), ServerError> {
    let table = inner.engine.copy_mode_table_for_pane(pane)?.to_owned();
    if matches!(
        inner
            .modes
            .pane_modes
            .get(&pane)
            .and_then(|modes| modes.last()),
        Some(PaneModeRequest::Panes(_))
    ) {
        pop_pane_mode(inner, pane);
    }
    inner
        .client_entry(client)
        .key_engine
        .get_or_insert_default()
        .switch_table(Some(table));
    let kill = inner.modes.pending_copy_kill.take() == Some(pane)
        || inner
            .client(client)
            .and_then(|c| c.copy_session.as_ref())
            .is_some_and(|session| session.pane == pane && !session.exiting && session.kill);
    let previous = inner
        .client_entry(client)
        .copy_session
        .replace(CopySession {
            pane,
            observed: false,
            scroll_exit: false,
            kill,
            refresh: false,
            unseen: false,
            sourced: false,
            exiting: false,
        });
    sync_client_pane_sink(inner, client, pane);
    if let Some(previous) = previous.filter(|previous| previous.pane != pane) {
        sync_client_pane_sink(inner, client, previous.pane);
    }
    Ok(())
}

enum ChooserKey {
    Pass,
    Key(zz_terminal::KeyInput),
    Claimed,
}

#[derive(Clone, Copy, Debug, Default)]
struct ChooserUnder {
    copy: bool,
    modes: usize,
}

fn chooser_under(inner: &ServerState, client: ClientId, pane: PaneId) -> ChooserUnder {
    ChooserUnder {
        copy: client_in_copy_mode(inner, client),
        modes: inner.modes.pane_modes.get(&pane).map_or(0, Vec::len),
    }
}

fn client_in_copy_mode(inner: &ServerState, client: ClientId) -> bool {
    inner
        .client(client)
        .and_then(|c| c.copy_session.as_ref())
        .is_some_and(|session| !session.exiting)
}

fn chooser_shown(inner: &ServerState, client: ClientId) -> bool {
    let Some(state) = inner.client(client) else {
        return false;
    };
    let Some(pane) = state
        .choose_tree
        .as_ref()
        .map(|chooser| chooser.source_pane)
        .or_else(|| {
            state
                .choose_buffer
                .as_ref()
                .map(|chooser| chooser.source_pane)
        })
    else {
        return false;
    };
    client_focused_window_for_attachment(inner, client)
        .and_then(|window| inner.engine.state.windows.get(&window))
        .is_some_and(|window| window.active_pane == pane)
        && (state.chooser_under.copy || !client_in_copy_mode(inner, client))
        && state
            .command_output
            .as_ref()
            .is_none_or(|output| output.parked)
        && inner.modes.pane_modes.get(&pane).map_or(0, Vec::len) <= state.chooser_under.modes
}

fn chooser_takes_keys(inner: &ServerState, client: ClientId) -> bool {
    chooser_shown(inner, client)
        && inner
            .client(client)
            .is_some_and(|c| c.display_panes.is_none() && c.command_prompt.is_none())
}

fn command_output_owns_pane(inner: &ServerState, client: ClientId, pane: PaneId) -> bool {
    inner
        .client(client)
        .and_then(|c| c.command_output.as_ref())
        .is_some_and(|output| output.pane == pane)
}

#[cfg(test)]
fn prefix_published(inner: &ServerState, client: ClientId) -> bool {
    inner
        .client(client)
        .and_then(|c| c.published_key_table.as_ref())
        .is_some_and(|(table, _)| table.as_deref() == Some("prefix"))
}

fn follow_command_output_focus(inner: &mut ServerState, client: ClientId, pane: PaneId) -> bool {
    let Some(output) = inner.client(client).and_then(|c| c.command_output.as_ref()) else {
        return false;
    };
    let focused = output.pane == pane;
    if focused != output.parked {
        return false;
    }
    let output_pane = output.pane;
    let previous = output.previous_key_table.clone();
    let current = inner
        .client(client)
        .and_then(|c| c.key_engine.as_ref())
        .and_then(|engine| engine.active_table())
        .map(str::to_owned);
    if focused {
        if current != previous {
            return false;
        }
        let Ok(table) = inner.engine.copy_mode_table_for_pane(output_pane) else {
            return false;
        };
        let table = table.to_owned();
        inner
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(Some(table));
    } else if matches!(current.as_deref(), Some("copy-mode" | "copy-mode-vi")) {
        inner
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(previous);
    }
    if let Some(output) = inner
        .client_mut(client)
        .and_then(|c| c.command_output.as_mut())
    {
        output.parked = !focused;
    }
    true
}

fn copy_mode_key_owners(inner: &ServerState, client: ClientId, pane: PaneId) -> Vec<ClientId> {
    let live = |session: &CopySession| session.pane == pane && !session.exiting;
    if inner
        .client(client)
        .and_then(|c| c.copy_session.as_ref())
        .is_some_and(live)
    {
        return vec![client];
    }
    inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.copy_session.as_ref().map(|value| (id, value)))
        .filter(|(_, session)| live(session))
        .map(|(owner, _)| *owner)
        .collect()
}

fn copy_mode_refresh_needed(inner: &ServerState) -> bool {
    inner.clients.values().any(|client| {
        client.copy_session.as_ref().is_some_and(|session| {
            session.refresh && !session.exiting && inner.terminals.contains_key(&session.pane)
        })
    })
}

fn copy_mode_refresh_ticks(inner: &ServerState) -> Vec<(ClientId, Arc<TerminalSession>)> {
    inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.copy_session.as_ref().map(|value| (id, value)))
        .filter(|(_, session)| session.refresh && !session.exiting)
        .filter_map(|(client, session)| {
            inner
                .terminals
                .get(&session.pane)
                .map(|terminal| (*client, Arc::clone(terminal)))
        })
        .collect()
}

fn terminal_view_action_arms_scroll_exit(action: &zz_terminal::TerminalViewAction) -> bool {
    matches!(
        action,
        zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::PageDownScrollExit)
            | zz_terminal::TerminalViewAction::CopyModeCounted {
                action: zz_terminal::CopyModeAction::PageDownScrollExit,
                ..
            }
    )
}

fn exit_copy_session(inner: &mut ServerState, client: ClientId) {
    if let Some(session) = inner.client_mut(client).and_then(|c| c.copy_session.take()) {
        sync_client_pane_sink(inner, client, session.pane);
    }
    inner
        .client_entry(client)
        .key_engine
        .get_or_insert_default()
        .switch_table(None);
}

fn begin_exit_copy_session(inner: &mut ServerState, client: ClientId) {
    if let Some(session) = inner
        .client_mut(client)
        .and_then(|c| c.copy_session.as_mut())
    {
        session.exiting = true;
    }
    inner
        .client_entry(client)
        .key_engine
        .get_or_insert_default()
        .switch_table(None);
}

fn copy_mode_format_facts(
    inner: &ServerState,
) -> BTreeMap<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>> {
    let mut panes: BTreeMap<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>> =
        BTreeMap::new();
    for (client, session) in inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.copy_session.as_ref().map(|session| (id, session)))
    {
        if session.exiting {
            continue;
        }
        let Some(facts) = inner
            .terminals
            .get(&session.pane)
            .and_then(|terminal| terminal.copy_mode_facts(TerminalViewId(client.0)))
        else {
            continue;
        };
        let name = inner
            .client(*client)
            .and_then(|c| c.name.as_ref())
            .cloned()
            .unwrap_or_default();
        panes.entry(session.pane).or_default().push((name, facts));
    }
    panes
}

fn unseen_change_panes(inner: &ServerState) -> BTreeSet<PaneId> {
    inner
        .clients
        .values()
        .filter_map(|c| c.copy_session.as_ref())
        .filter(|session| !session.exiting && session.unseen)
        .map(|session| session.pane)
        .collect()
}

fn active_copy_mode_panes(inner: &ServerState) -> BTreeSet<PaneId> {
    inner
        .clients
        .values()
        .filter_map(|c| c.copy_session.as_ref())
        .filter(|session| !session.exiting)
        .map(|session| session.pane)
        .collect()
}

fn terminal_view_action_enters_copy_mode(action: &zz_terminal::TerminalViewAction) -> bool {
    matches!(
        action,
        zz_terminal::TerminalViewAction::EnterCopyMode
            | zz_terminal::TerminalViewAction::EnterCopyModeScrollExit
            | zz_terminal::TerminalViewAction::EnterCopyModeWith { .. }
    )
}

fn terminal_view_action_exits_copy_mode(action: &zz_terminal::TerminalViewAction) -> bool {
    match action {
        zz_terminal::TerminalViewAction::ClearHistory
        | zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel)
        | zz_terminal::TerminalViewAction::CopyModeCounted {
            action: zz_terminal::CopyModeAction::Cancel,
            ..
        } => true,
        zz_terminal::TerminalViewAction::CopyMode(
            zz_terminal::CopyModeAction::CopySelection(copy)
            | zz_terminal::CopyModeAction::CopyEndOfLine(copy)
            | zz_terminal::CopyModeAction::CopyLine(copy),
        )
        | zz_terminal::TerminalViewAction::CopyModeCounted {
            action:
                zz_terminal::CopyModeAction::CopySelection(copy)
                | zz_terminal::CopyModeAction::CopyEndOfLine(copy)
                | zz_terminal::CopyModeAction::CopyLine(copy),
            ..
        } => copy.cancel,
        _ => false,
    }
}

fn unfocused_copy_sessions(inner: &mut ServerState) -> Vec<(ClientId, Arc<TerminalSession>)> {
    let stale = inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.copy_session.as_ref().map(|value| (id, value)))
        .filter(|(client, session)| client_context_pane(inner, **client) != Some(session.pane))
        .map(|(client, session)| (*client, session.pane))
        .collect::<Vec<_>>();
    stale
        .into_iter()
        .filter_map(|(client, pane)| {
            exit_copy_session(inner, client);
            Some((client, inner.terminals.get(&pane).cloned()?))
        })
        .collect()
}

fn client_attached_session(inner: &ServerState, client: ClientId) -> Option<SessionId> {
    client_attached_session_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn client_attached_session_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> Option<SessionId> {
    inner
        .attached
        .iter()
        .find_map(|(session, clients)| clients.contains(&client).then_some(*session))
}

fn client_tty_fact(capabilities: &[String]) -> Option<String> {
    capabilities.iter().find_map(|capability| {
        capability
            .strip_prefix(ClientHello::CLIENT_TTY_CAPABILITY_PREFIX)
            .filter(|tty| !tty.is_empty())
            .map(str::to_owned)
    })
}

fn client_nested_fact(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == ClientHello::CLIENT_NESTED_CAPABILITY)
}

fn client_remote_fact(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == ClientHello::CLIENT_REMOTE_CAPABILITY)
}

fn client_utf8_fact(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == ClientHello::CLIENT_UTF8_CAPABILITY)
}

fn client_features_fact(capabilities: &[String]) -> u32 {
    terminal_feature_mask(capabilities.iter().filter_map(|capability| {
        capability.strip_prefix(ClientHello::CLIENT_FEATURES_CAPABILITY_PREFIX)
    }))
}

fn client_size_fact(capabilities: &[String]) -> Option<(u16, u16)> {
    capabilities.iter().find_map(|capability| {
        let value = capability.strip_prefix(ClientHello::CLIENT_SIZE_CAPABILITY_PREFIX)?;
        let (columns, rows) = value.split_once('x')?;
        let columns = columns.parse::<u16>().ok().filter(|columns| *columns > 0)?;
        let rows = rows.parse::<u16>().ok().filter(|rows| *rows > 0)?;
        Some((columns, rows))
    })
}

fn client_working_directory_fact(working_directory: Option<&ClientPath>) -> Option<PathBuf> {
    working_directory
        .and_then(ClientPath::to_path_buf)
        .filter(|working_directory| working_directory.is_absolute())
}

fn client_environment_fact(entries: &[RawText]) -> Arc<ClientEnvironmentBlob> {
    Arc::new(ClientEnvironmentBlob::from_map(
        entries
            .iter()
            .filter_map(|entry| entry.split_once_byte(b'='))
            .collect(),
    ))
}

fn effective_mux_options(inner: &ServerState, client: ClientId) -> MuxOptions {
    let mut options = inner.config.mux_options.clone();
    let session = client_attached_session(inner, client);
    let mouse = inner.engine.effective_mouse(session);
    let source = options
        .get(MuxOptionKey::Mouse)
        .map_or(MuxOptionSource::Default, |value| value.source);
    options.set(
        MuxOptionKey::Mouse,
        if mouse { "on" } else { "off" },
        source,
    );
    let focus_follows_mouse = inner.engine.effective_focus_follows_mouse(session);
    let source = options
        .get(MuxOptionKey::FocusFollowsMouse)
        .map_or(MuxOptionSource::Default, |value| value.source);
    options.set(
        MuxOptionKey::FocusFollowsMouse,
        if focus_follows_mouse { "on" } else { "off" },
        source,
    );
    options
}

fn best_client_on_session(inner: &ServerState, session: SessionId) -> Option<ClientId> {
    inner
        .attachment
        .attached
        .get(&session)?
        .iter()
        .copied()
        .max_by_key(|client| {
            (
                inner
                    .client(*client)
                    .and_then(|c| c.activity)
                    .unwrap_or_default(),
                Reverse(client.0),
            )
        })
}

fn best_attached_client(inner: &ServerState) -> Option<ClientId> {
    inner
        .attachment
        .attached
        .values()
        .flatten()
        .copied()
        .max_by_key(|client| {
            (
                inner
                    .client(*client)
                    .and_then(|c| c.activity)
                    .unwrap_or_default(),
                Reverse(client.0),
            )
        })
}

fn best_attached_session(inner: &ServerState) -> Option<SessionId> {
    inner
        .attachment
        .attached
        .iter()
        .filter(|(session, clients)| {
            !clients.is_empty() && inner.engine.state.sessions.contains_key(session)
        })
        .map(|(session, _)| *session)
        .max_by_key(|session| {
            let state = &inner.engine.state.sessions[session];
            (state.sort_activity, state.sort_created, state.name.clone())
        })
}

fn resolve_client_target(
    inner: &ServerState,
    invoking_client: ClientId,
    kind: ClientKind,
    target: Option<&str>,
) -> Result<ClientId, ServerError> {
    if let Some(target) = target {
        return find_attached_client(inner, target).ok_or_else(|| client_target_error(target));
    }
    if matches!(kind, ClientKind::Interactive | ClientKind::Control)
        && client_attached_session(inner, invoking_client).is_some()
    {
        return Ok(invoking_client);
    }
    current_client(inner, invoking_client)
        .ok_or_else(|| ServerError::InvalidCommand("no current client".to_owned()))
}

fn current_client(inner: &ServerState, invoking_client: ClientId) -> Option<ClientId> {
    if let Some(session) = inner
        .client(invoking_client)
        .and_then(|c| c.origin.as_ref())
        .and_then(|pane| inner.engine.state.window_for_pane(*pane))
        .and_then(|window| inner.engine.state.windows.get(&window))
        .map(|window| window.session)
        && let Some(client) = best_client_on_session(inner, session)
    {
        return Some(client);
    }
    best_attached_session(inner).and_then(|session| best_client_on_session(inner, session))
}

fn normalized_client_target(target: &str) -> &str {
    target.strip_suffix(':').unwrap_or(target)
}

fn attached_client_matches(
    inner: &ServerState,
    client: ClientId,
    target: &str,
    native_aliases: bool,
) -> bool {
    inner
        .client(client)
        .and_then(|c| c.name.as_ref())
        .is_some_and(|name| name == target)
        || client_format_name(inner, client) == target
        || format!("device-{}", client.0) == target
        || inner
            .client(client)
            .and_then(|c| c.tty.as_ref())
            .is_some_and(|tty| {
                tty == target
                    || tty
                        .strip_prefix("/dev/")
                        .is_some_and(|device| device == target)
            })
        || native_aliases
            && (target == client.0.to_string() || target == format!("client-{}", client.0))
}

fn find_attached_client_with_aliases(
    inner: &ServerState,
    target: &str,
    native_aliases: bool,
) -> Option<ClientId> {
    let target = normalized_client_target(target);
    inner
        .attachment
        .attached
        .values()
        .flatten()
        .copied()
        .filter(|client| attached_client_matches(inner, *client, target, native_aliases))
        .min()
}

fn find_attached_client(inner: &ServerState, target: &str) -> Option<ClientId> {
    find_attached_client_with_aliases(inner, target, false)
}

fn client_target_error(target: &str) -> ServerError {
    ServerError::InvalidCommand(format!(
        "can't find client: {}",
        normalized_client_target(target)
    ))
}

fn nested_attach_refusal(inner: &ServerState, client: ClientId) -> Option<ServerError> {
    if !inner.client(client).is_some_and(|c| c.nested) {
        return None;
    }
    let tty = inner.client(client).and_then(|c| c.tty.as_ref())?;
    if inner.engine.state.sessions.is_empty() {
        return None;
    }
    inner
        .terminals
        .iter()
        .filter(|(pane, _)| !inner.panes.popup_panes.contains(pane))
        .any(|(_, terminal)| {
            terminal
                .tty()
                .is_some_and(|pane_tty| pane_tty.as_os_str() == tty.as_str())
        })
        .then(|| {
            ServerError::InvalidCommand(
                "sessions should be nested with care, unset $TMUX to force".to_owned(),
            )
        })
}

#[cfg(test)]
fn format_client_flags(inner: &ServerState, client: ClientId) -> String {
    format_client_flags_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn format_client_flags_from_source(inner: &ClientFormatSource<'_>, client: ClientId) -> String {
    let mut flags = Vec::new();
    let requested = inner.client_flags.get(client);
    if client_attached_session_from_source(inner, client).is_some() {
        flags.push("attached".to_owned());
    }
    if inner
        .clients
        .get(&client)
        .and_then(|c| c.focused)
        .unwrap_or(true)
    {
        flags.push("focused".to_owned());
    }
    if inner.clients.get(&client).and_then(|c| c.kind) == Some(ClientKind::Control) {
        flags.push("control-mode".to_owned());
    }
    if client_ignores_size_from_source(inner, client) {
        flags.push("ignore-size".to_owned());
    }
    if requested.no_detach_on_destroy {
        flags.push("no-detach-on-destroy".to_owned());
    }
    if inner.clients.get(&client).and_then(|c| c.kind) == Some(ClientKind::Control)
        && let Some(output) = inner
            .clients
            .get(&client)
            .and_then(|c| c.control_output.as_ref())
    {
        if output.no_output {
            flags.push("no-output".to_owned());
        }
        if output.wait_exit {
            flags.push("wait-exit".to_owned());
        }
        if output.new_layouts {
            flags.push("new-layouts".to_owned());
        }
        if let Some(pause_after_ms) = output.pause_after_ms {
            flags.push(format!("pause-after={}", pause_after_ms / 1000));
        }
    }
    if requested.read_only {
        flags.push("read-only".to_owned());
    }
    if client_uses_utf8_from_source(inner, client) {
        flags.push("UTF-8".to_owned());
    }
    flags.join(",")
}

fn client_environment_value<'a>(
    inner: &'a ServerState,
    client: ClientId,
    name: &str,
) -> Option<&'a str> {
    client_environment_value_from_source(&ClientFormatSource::from_inner(inner), client, name)
}

fn client_environment_value_from_source<'a>(
    inner: &ClientFormatSource<'a>,
    client: ClientId,
    name: &str,
) -> Option<&'a str> {
    inner
        .fields
        .clients
        .get(&client)
        .and_then(|c| c.environment.as_ref())?
        .map()
        .get(name)
        .map(RawText::as_str)
}

fn client_uses_utf8_from_source(inner: &ClientFormatSource<'_>, client: ClientId) -> bool {
    if inner.clients.get(&client).is_some_and(|c| c.utf8) {
        return true;
    }
    if client_environment_value_from_source(inner, client, "TMUX")
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|name| {
            client_environment_value_from_source(inner, client, name)
                .filter(|value| !value.is_empty())
        })
        .is_some_and(|locale| {
            let locale = locale.to_ascii_lowercase();
            locale.contains("utf-8") || locale.contains("utf8")
        })
}

fn client_colour_count_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> Option<u32> {
    if inner.clients.get(&client).and_then(|c| c.kind) != Some(ClientKind::Interactive)
        || !inner.clients.get(&client).is_some_and(|c| c.has_terminal)
    {
        return None;
    }
    client_colour_count_with_from_source(
        inner,
        client,
        client_feature_mask_from_source(inner, client),
    )
}

fn client_terminal_features_option_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> Vec<String> {
    let mut features = inner.engine.terminal_features_option();
    let term = client_environment_value_from_source(inner, client, "TERM").unwrap_or_default();
    let learned = terminal_features_list(
        inner
            .clients
            .get(&client)
            .and_then(|c| c.features)
            .unwrap_or(0),
    );
    if !term.is_empty() && !learned.is_empty() {
        features.push(format!("{term}:{}", learned.replace(',', ":")));
    }
    features
}

fn client_negotiated_features_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> String {
    terminal_features_list(
        inner
            .clients
            .get(&client)
            .and_then(|c| c.features)
            .unwrap_or(0),
    )
}

#[cfg(all(test, unix))]
fn client_feature_mask(inner: &ServerState, client: ClientId) -> u32 {
    client_feature_mask_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn client_feature_mask_from_source(inner: &ClientFormatSource<'_>, client: ClientId) -> u32 {
    let mut features = inner
        .clients
        .get(&client)
        .and_then(|c| c.features)
        .unwrap_or(0);
    if let Some(term) = client_terminal_facts(
        client_environment_value_from_source(inner, client, "TERM").unwrap_or_default(),
        client_environment_value_from_source(inner, client, "COLORTERM"),
        &client_negotiated_features_from_source(inner, client),
        &inner.engine.terminal_features_option(),
        &inner.engine.terminal_overrides_option(),
    ) {
        features |= terminal_feature_mask(term.requested_features());
        features &= !terminal_feature_mask(term.removed_features());
    }
    features
}

type TerminalNegotiation = (Vec<String>, Vec<String>, Vec<String>);

fn client_terminal_negotiation(
    inner: &ServerState,
    client: ClientId,
) -> Option<TerminalNegotiation> {
    let source = ClientFormatSource::from_inner(inner);
    client_colour_count_from_source(&source, client)?;
    let features = terminal_features_list(client_feature_mask_from_source(&source, client));
    let application_escape = client_terminal_facts(
        client_environment_value_from_source(&source, client, "TERM").unwrap_or_default(),
        client_environment_value_from_source(&source, client, "COLORTERM"),
        &features,
        &inner.engine.terminal_features_option(),
        &inner.engine.terminal_overrides_option(),
    )
    .map(|term| {
        ["Enesc", "Dsesc"]
            .map(|name| term.string_capability(name).to_owned())
            .to_vec()
    })
    .unwrap_or_default();
    Some((
        features
            .split(',')
            .filter(|feature| !feature.is_empty())
            .map(str::to_owned)
            .collect(),
        inner.engine.user_keys_option(),
        application_escape,
    ))
}

fn take_terminal_negotiation(inner: &mut ServerState, client: ClientId) -> Option<EventPayload> {
    let negotiation = client_terminal_negotiation(inner, client)?;
    if inner
        .client(client)
        .and_then(|c| c.published_terminal_negotiation.as_ref())
        == Some(&negotiation)
    {
        return None;
    }
    inner
        .client_entry(client)
        .published_terminal_negotiation
        .replace(negotiation.clone());
    let (features, user_keys, application_escape) = negotiation;
    Some(EventPayload::TerminalNegotiation {
        features,
        user_keys,
        application_escape,
    })
}

fn client_colour_count_with_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    requested: u32,
) -> Option<u32> {
    if inner.clients.get(&client).and_then(|c| c.kind) != Some(ClientKind::Interactive)
        || !inner.clients.get(&client).is_some_and(|c| c.has_terminal)
    {
        return None;
    }
    let term = client_environment_value_from_source(inner, client, "TERM").unwrap_or_default();
    let colour_term =
        client_environment_value_from_source(inner, client, "COLORTERM").unwrap_or_default();
    Some(terminal_colour_count(term, colour_term, requested))
}

#[cfg(test)]
fn client_term_features(inner: &ServerState, client: ClientId) -> String {
    client_term_features_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn client_term_features_from_source(inner: &ClientFormatSource<'_>, client: ClientId) -> String {
    if client_colour_count_from_source(inner, client).is_none() {
        return String::new();
    }
    terminal_features_list(client_feature_mask_from_source(inner, client))
}

fn client_format_geometry_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    window: WindowId,
) -> Option<TerminalGeometry> {
    let pane = inner.engine.state.windows.get(&window)?.active_pane;
    inner
        .terminal_geometries
        .get(&pane)
        .and_then(|geometries| geometries.get(&client))
        .copied()
        .or_else(|| {
            inner
                .terminal_geometries
                .values()
                .find_map(|geometries| geometries.get(&client).copied())
        })
}

fn client_format_time(time: Option<u64>) -> String {
    time.filter(|time| *time != 0)
        .map_or_else(String::new, |time| time.to_string())
}

fn client_ignores_size(inner: &ServerState, client: ClientId) -> bool {
    client_ignores_size_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn client_ignores_size_from_source(inner: &ClientFormatSource<'_>, client: ClientId) -> bool {
    inner.client_flags.get(client).ignore_size
}

fn unignored_attached_sizing_client_exists(inner: &ServerState) -> bool {
    inner.attachment.attached.values().flatten().any(|client| {
        matches!(
            inner.client(*client).and_then(|c| c.kind),
            Some(ClientKind::Interactive | ClientKind::Control)
        ) && !client_ignores_size(inner, *client)
    })
}

fn client_is_sizing_candidate(
    inner: &ServerState,
    client: ClientId,
    suppress_ignored: bool,
) -> bool {
    matches!(
        inner.client(client).and_then(|c| c.kind),
        Some(ClientKind::Interactive | ClientKind::Control)
    ) && (!suppress_ignored || !client_ignores_size(inner, client))
}

fn interactive_client_window_extent(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    window: WindowId,
) -> Option<(u16, u16)> {
    interactive_client_window_extent_from_source(
        &ClientFormatSource::from_inner(inner),
        client,
        session,
        window,
    )
}

fn interactive_client_window_extent_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: SessionId,
    window: WindowId,
) -> Option<(u16, u16)> {
    if let Some((columns, rows)) = inner.clients.get(&client).and_then(|c| c.size) {
        let lines = u16::from(inner.engine.status_rows_for_session(Some(session)));
        let status_rows = if rows <= lines { 0 } else { lines };
        return Some((columns.max(1), rows.saturating_sub(status_rows).max(1)));
    }
    let window_state = inner.engine.state.windows.get(&window)?;
    std::iter::once(window_state.active_pane)
        .chain(
            window_state
                .panes
                .keys()
                .copied()
                .filter(|pane| *pane != window_state.active_pane),
        )
        .find_map(|pane| {
            let geometry = inner.terminal_geometries.get(&pane)?.get(&client)?;
            inner
                .engine
                .window_extent_for_pane_geometry(pane, geometry.columns, geometry.rows)
        })
}

fn control_client_geometry(
    inner: &ServerState,
    client: ClientId,
    window: WindowId,
) -> Option<TerminalGeometry> {
    control_client_geometry_from_source(&ClientFormatSource::from_inner(inner), client, window)
}

fn control_client_geometry_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    window: WindowId,
) -> Option<TerminalGeometry> {
    let output = inner
        .clients
        .get(&client)
        .and_then(|c| c.control_output.as_ref())?;
    output
        .window_geometries
        .get(&window)
        .copied()
        .or(output.geometry)
}

fn control_client_sized_panes(inner: &ServerState, client: ClientId) -> BTreeSet<PaneId> {
    let Some(output) = inner.client(client).and_then(|c| c.control_output.as_ref()) else {
        return BTreeSet::new();
    };
    let Some(session) = client_attached_session(inner, client)
        .and_then(|session| inner.engine.state.sessions.get(&session))
    else {
        return BTreeSet::new();
    };
    session
        .windows
        .iter()
        .filter(|window| output.geometry.is_some() || output.window_geometries.contains_key(window))
        .filter_map(|window| inner.engine.state.windows.get(window))
        .flat_map(|window| window.panes.keys().copied())
        .collect()
}

fn client_format_facts(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
) -> ClientFormatFacts {
    client_format_facts_from_source(&ClientFormatSource::from_inner(inner), client, session)
}

#[derive(Clone, Copy, Default)]
struct ClientFactSelection {
    full: bool,
    activity: bool,
    cell_height: bool,
    cell_width: bool,
    colours: bool,
    control_mode: bool,
    created: bool,
    discarded: bool,
    flags: bool,
    height: bool,
    key_table: bool,
    last_session: bool,
    name: bool,
    pid: bool,
    prefix: bool,
    readonly: bool,
    session: bool,
    termfeatures: bool,
    termname: bool,
    termtype: bool,
    theme: bool,
    tty: bool,
    uid: bool,
    user: bool,
    utf8: bool,
    width: bool,
    written: bool,
    viewport: bool,
}

#[cfg(test)]
thread_local! {
    static FORMAT_FACT_SELECTION_BUILDS: Cell<usize> = const { Cell::new(0) };
}

impl ClientFactSelection {
    fn complete() -> Self {
        Self {
            full: true,
            ..Self::default()
        }
    }

    fn from_references(references: &BTreeSet<String>) -> Self {
        #[cfg(test)]
        FORMAT_FACT_SELECTION_BUILDS.with(|count| count.set(count.get() + 1));
        let mut selection = Self {
            width: true,
            ..Self::default()
        };
        for name in references {
            match name.as_str() {
                "*" => return Self::complete(),
                "client_activity" => selection.activity = true,
                "client_cell_height" | "window_cell_height" => selection.cell_height = true,
                "client_cell_width" | "window_cell_width" => selection.cell_width = true,
                "client_colours" => selection.colours = true,
                "client_control_mode" => selection.control_mode = true,
                "client_created" => selection.created = true,
                "client_discarded" => selection.discarded = true,
                "client_flags" => selection.flags = true,
                "client_height" => selection.height = true,
                "client_key_table" => selection.key_table = true,
                "client_last_session" => selection.last_session = true,
                "client_name" => selection.name = true,
                "client_pid" => selection.pid = true,
                "client_prefix" => selection.prefix = true,
                "client_readonly" => selection.readonly = true,
                "client_session" => selection.session = true,
                "client_termfeatures" => selection.termfeatures = true,
                "client_termname" => selection.termname = true,
                "client_termtype" => selection.termtype = true,
                "client_theme" => selection.theme = true,
                "client_tty" => selection.tty = true,
                "client_uid" => selection.uid = true,
                "client_user" => selection.user = true,
                "client_utf8" => selection.utf8 = true,
                "client_width" => selection.width = true,
                "client_written" => selection.written = true,
                "window_bigger" | "window_offset_x" | "window_offset_y" => {
                    selection.viewport = true;
                }
                name if name.starts_with("mode_") => return Self::complete(),
                _ => {}
            }
        }
        selection
    }
}

fn client_format_facts_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: SessionId,
) -> ClientFormatFacts {
    client_format_facts_requested(inner, client, session, &ClientFactSelection::complete())
}

#[cfg(test)]
fn selected_client_format_facts(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    references: &BTreeSet<String>,
) -> ClientFormatFacts {
    let border_references = inner.engine.cached_format_references_for_templates([
        "#{E:pane-border-style}",
        "#{E:pane-active-border-style}",
    ]);
    let references = references
        .iter()
        .chain(border_references.iter())
        .cloned()
        .collect();
    selected_client_format_facts_with_references(inner, client, session, &references)
}

fn selected_client_format_facts_with_references(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    references: &BTreeSet<String>,
) -> ClientFormatFacts {
    selected_client_format_facts_with_selection(
        inner,
        client,
        session,
        &ClientFactSelection::from_references(references),
    )
}

fn selected_client_format_facts_with_selection(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    selection: &ClientFactSelection,
) -> ClientFormatFacts {
    client_format_facts_requested(
        &ClientFormatSource::from_inner(inner),
        client,
        session,
        selection,
    )
}

fn client_format_facts_requested(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: SessionId,
    selection: &ClientFactSelection,
) -> ClientFormatFacts {
    macro_rules! capture {
        ($field:ident, $value:expr) => {
            if selection.full || selection.$field {
                $value
            } else {
                String::new()
            }
        };
    }
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return ClientFormatFacts::default();
    };
    let registered = inner.clients.get(&client);
    let kind = registered.and_then(|c| c.kind);
    let has_terminal =
        kind == Some(ClientKind::Interactive) && registered.is_some_and(|c| c.has_terminal);
    let cell_height = selection.full || selection.cell_height;
    let cell_width = selection.full || selection.cell_width;
    let viewport = selection.full || selection.viewport;
    let window = (cell_height || cell_width || viewport || kind == Some(ClientKind::Control))
        .then(|| client_focused_window_from_source(inner, client, session_state));
    let control_geometry = (kind == Some(ClientKind::Control))
        .then(|| control_client_geometry_from_source(inner, client, window.unwrap()))
        .flatten();
    let terminal_geometry = (has_terminal && (cell_height || cell_width))
        .then(|| client_format_geometry_from_source(inner, client, window.unwrap()))
        .flatten();
    let retained_size = registered.and_then(|c| c.size);
    let width = if kind == Some(ClientKind::Control) {
        control_geometry.map_or(80, |geometry| geometry.columns)
    } else {
        retained_size.map_or(80, |size| size.0)
    };
    let (key_table, prefix) = if selection.full || selection.key_table || selection.prefix {
        let default_key_table = inner.engine.key_table_for_session(session);
        let key_table = inner
            .clients
            .get(&client)
            .and_then(|c| c.key_engine.as_ref())
            .and_then(KeyEngine::active_table)
            .unwrap_or(default_key_table.as_str());
        (
            capture!(key_table, key_table.to_owned()),
            capture!(
                prefix,
                usize::from(key_table != default_key_table).to_string()
            ),
        )
    } else {
        (String::new(), String::new())
    };
    let (written, discarded) = if selection.full || selection.written || selection.discarded {
        inner
            .clients
            .get(&client)
            .and_then(|c| c.subscriber.as_ref())
            .map_or((0, 0), |subscriber| subscriber.stats())
    } else {
        (0, 0)
    };
    ClientFormatFacts {
        activity: capture!(
            activity,
            client_format_time(registered.and_then(|c| c.activity_time))
        ),
        cell_height: if cell_height {
            terminal_geometry
                .map_or_else(
                    || has_terminal.then_some(0),
                    |geometry| Some(geometry.cell_height_px),
                )
                .map_or_else(String::new, |height| height.to_string())
        } else {
            String::new()
        },
        cell_width: if cell_width {
            terminal_geometry
                .map_or_else(
                    || has_terminal.then_some(0),
                    |geometry| Some(geometry.cell_width_px),
                )
                .map_or_else(String::new, |width| width.to_string())
        } else {
            String::new()
        },
        colours: capture!(
            colours,
            client_colour_count_from_source(inner, client)
                .map_or_else(String::new, |colours| colours.to_string())
        ),
        control_mode: capture!(
            control_mode,
            usize::from(kind == Some(ClientKind::Control)).to_string()
        ),
        created: capture!(
            created,
            client_format_time(registered.and_then(|c| c.created_time))
        ),
        discarded: capture!(discarded, discarded.to_string()),
        flags: capture!(flags, format_client_flags_from_source(inner, client)),
        height: capture!(
            height,
            has_terminal
                .then(|| retained_size.map_or(24, |size| size.1))
                .map_or_else(String::new, |height| height.to_string())
        ),
        key_table,
        last_session: capture!(
            last_session,
            registered
                .and_then(|c| c.last_session.as_ref())
                .and_then(|last| inner.engine.state.sessions.get(last))
                .map(|session| session.name.clone())
                .unwrap_or_default()
        ),
        name: capture!(name, client_format_name_from_source(inner, client)),
        pid: capture!(
            pid,
            registered
                .and_then(|c| c.pid.as_ref())
                .filter(|pid| **pid != 0)
                .map(u32::to_string)
                .unwrap_or_default()
        ),
        prefix,
        readonly: capture!(
            readonly,
            usize::from(inner.client_flags.contains(client)).to_string()
        ),
        session: capture!(session, session_state.name.clone()),
        termfeatures: capture!(
            termfeatures,
            client_term_features_from_source(inner, client)
        ),
        termname: capture!(
            termname,
            client_environment_value_from_source(inner, client, "TERM")
                .filter(|term| !term.is_empty())
                .unwrap_or("unknown")
                .to_owned()
        ),
        termtype: capture!(
            termtype,
            registered
                .and_then(|c| c.terminal_type.as_ref())
                .cloned()
                .unwrap_or_default()
        ),
        theme: capture!(
            theme,
            registered
                .and_then(|c| c.color_scheme.as_ref())
                .map(|scheme| scheme.as_str().to_owned())
                .unwrap_or_default()
        ),
        tty: capture!(
            tty,
            registered
                .and_then(|c| c.tty.as_ref())
                .cloned()
                .unwrap_or_default()
        ),
        uid: capture!(uid, inner.engine.format_uid().to_owned()),
        user: capture!(user, inner.engine.format_user().to_owned()),
        utf8: capture!(
            utf8,
            usize::from(client_uses_utf8_from_source(inner, client)).to_string()
        ),
        width: capture!(width, width.to_string()),
        written: capture!(written, written.to_string()),
        line: 0,
        environment: selection
            .full
            .then(|| registered.and_then(|c| c.environment.as_ref()).cloned())
            .flatten(),
        terminal: (selection.full && has_terminal)
            .then(|| {
                client_terminal_facts(
                    client_environment_value_from_source(inner, client, "TERM").unwrap_or_default(),
                    client_environment_value_from_source(inner, client, "COLORTERM"),
                    &client_negotiated_features_from_source(inner, client),
                    &client_terminal_features_option_from_source(inner, client),
                    &inner.engine.terminal_overrides_option(),
                )
            })
            .flatten(),
        viewport: (has_terminal && viewport)
            .then(|| client_viewport_facts_from_source(inner, client, session, window.unwrap()))
            .flatten(),
    }
}

fn client_viewport_facts_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: SessionId,
    window: WindowId,
) -> Option<ClientViewportFacts> {
    let (columns, rows) =
        interactive_client_window_extent_from_source(inner, client, session, window)?;
    let pane = inner.engine.state.windows.get(&window)?.active_pane;
    let geometry = inner
        .engine
        .pane_format_geometry(Some(session), Some(window), Some(pane));
    let cursor = inner.terminals.get(&pane).and_then(|terminal| {
        let viewport = terminal.latest_viewport();
        let (column, row) = if (viewport.columns, viewport.rows) == terminal.size() {
            let cursor = viewport.cursor.filter(|cursor| cursor.visible())?;
            (cursor.column(), cursor.row())
        } else {
            let facts = terminal.facts();
            if facts.cursor_hidden {
                return None;
            }
            (facts.cursor_x, facts.cursor_y)
        };
        Some((
            geometry.pane_left?.saturating_add(column),
            geometry.pane_top?.saturating_add(row),
        ))
    });
    Some(ClientViewportFacts {
        columns,
        rows,
        window_width: geometry.window_width?,
        window_height: geometry.window_height?,
        cursor,
    })
}

fn server_log_client_name(inner: &ServerState, client: ClientId) -> String {
    inner
        .client(client)
        .and_then(|c| c.tty.as_ref())
        .filter(|tty| !tty.is_empty())
        .or_else(|| {
            inner
                .client(client)
                .and_then(|c| c.name.as_ref())
                .filter(|name| !name.is_empty())
        })
        .cloned()
        .unwrap_or_else(|| format!("device-{}", client.0))
}

fn client_format_name(inner: &ServerState, client: ClientId) -> String {
    client_format_name_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn client_format_name_from_source(inner: &ClientFormatSource<'_>, client: ClientId) -> String {
    inner
        .clients
        .get(&client)
        .and_then(|c| c.tty.as_ref())
        .filter(|tty| !tty.is_empty())
        .cloned()
        .or_else(|| {
            inner
                .clients
                .get(&client)
                .and_then(|c| c.name.as_ref())
                .filter(|name| !name.is_empty())
                .cloned()
        })
        .unwrap_or_else(|| {
            let pid = inner
                .clients
                .get(&client)
                .and_then(|c| c.pid)
                .unwrap_or_default();
            if pid == 0 {
                format!("device-{}", client.0)
            } else {
                format!("client-{pid}")
            }
        })
}

fn client_focused_window(
    inner: &ServerState,
    client: ClientId,
    session: &zz_mux::Session,
) -> WindowId {
    client_focused_window_from_source(&ClientFormatSource::from_inner(inner), client, session)
}

fn client_focused_window_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: &zz_mux::Session,
) -> WindowId {
    inner
        .clients
        .get(&client)
        .and_then(|c| c.focused_window.as_ref())
        .copied()
        .filter(|focused| session.windows.contains(focused))
        .unwrap_or(session.active_window)
}

#[derive(Default)]
struct PaneFocusProbe {
    window_active_panes: BTreeMap<WindowId, PaneId>,
    session_windows: BTreeMap<SessionId, WindowId>,
    client_sessions: BTreeMap<ClientId, SessionId>,
    client_focused: BTreeMap<ClientId, bool>,
    client_overlays: BTreeSet<ClientId>,
}

#[derive(Default)]
struct PaneFocusEvents {
    leading: Vec<PendingHookEvent>,
    after_window_pane_changed: Vec<(WindowId, PendingHookEvent)>,
}

impl PaneFocusEvents {
    fn is_empty(&self) -> bool {
        self.leading.is_empty() && self.after_window_pane_changed.is_empty()
    }
}

fn capture_pane_focus_probe(inner: &ServerState) -> PaneFocusProbe {
    PaneFocusProbe {
        window_active_panes: inner
            .engine
            .state
            .windows
            .iter()
            .map(|(window, state)| (*window, state.active_pane))
            .collect(),
        session_windows: inner
            .engine
            .state
            .sessions
            .iter()
            .map(|(session, state)| (*session, state.active_window))
            .collect(),
        ..capture_client_focus_probe(inner)
    }
}

fn capture_client_focus_probe(inner: &ServerState) -> PaneFocusProbe {
    PaneFocusProbe {
        window_active_panes: BTreeMap::new(),
        session_windows: BTreeMap::new(),
        client_sessions: inner
            .attachment
            .attached
            .iter()
            .flat_map(|(session, clients)| clients.iter().map(|client| (*client, *session)))
            .collect(),
        client_focused: inner
            .clients
            .iter()
            .filter_map(|(id, client)| client.focused.map(|focused| (*id, focused)))
            .collect(),
        client_overlays: inner
            .clients
            .iter()
            .filter_map(|(id, client)| client.kind.map(|_| *id))
            .filter(|client| any_overlay_present(inner, *client))
            .collect(),
    }
}

fn pane_is_focused(inner: &ServerState, pane: PaneId) -> bool {
    let Some(window) = inner.engine.state.window_for_pane(pane) else {
        return false;
    };
    if inner
        .engine
        .state
        .windows
        .get(&window)
        .map(|state| state.active_pane)
        != Some(pane)
    {
        return false;
    }
    inner
        .clients
        .iter()
        .filter_map(|(id, client)| (client.focused == Some(true)).then_some(*id))
        .filter(|client| !any_overlay_present(inner, *client))
        .any(|client| client_focused_window_for_attachment(inner, client) == Some(window))
}

fn pane_focus_transition(inner: &mut ServerState, pane: PaneId) -> Option<PendingHookEvent> {
    if inner.engine.state.pane(pane).is_none_or(|state| state.dead) {
        return None;
    }
    let focused = pane_is_focused(inner, pane);
    if focused == inner.attachment.pane_focus.contains(&pane) {
        return None;
    }
    let name = if focused {
        inner.attachment.pane_focus.insert(pane);
        "pane-focus-in"
    } else {
        inner.attachment.pane_focus.remove(&pane);
        "pane-focus-out"
    };
    PendingHookEvent::live_pane(name, pane, &inner.engine)
}

fn session_active_pane(inner: &ServerState, session: SessionId) -> Option<PaneId> {
    let window = inner.engine.state.sessions.get(&session)?.active_window;
    Some(inner.engine.state.windows.get(&window)?.active_pane)
}

fn client_active_pane(inner: &ServerState, client: ClientId) -> Option<PaneId> {
    let window = client_focused_window_for_attachment(inner, client)?;
    Some(inner.engine.state.windows.get(&window)?.active_pane)
}

fn pane_focus_candidates(
    inner: &ServerState,
    before: &PaneFocusProbe,
) -> (Vec<PaneId>, Vec<(WindowId, PaneId)>) {
    if no_pane_can_hold_focus(inner) {
        return (Vec::new(), Vec::new());
    }
    let gated = inner.engine.focus_events();
    let mut leading = Vec::new();
    let mut removals = Vec::new();
    for client in before
        .client_overlays
        .iter()
        .copied()
        .chain(
            inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.kind.map(|_| *id))
                .filter(|client| any_overlay_present(inner, *client)),
        )
        .collect::<BTreeSet<_>>()
    {
        if before.client_overlays.contains(&client) != any_overlay_present(inner, client) {
            leading.extend(client_active_pane(inner, client));
        }
    }
    for (client, session) in inner
        .attachment
        .attached
        .iter()
        .flat_map(|(session, clients)| clients.iter().map(|client| (*client, Some(*session))))
        .chain(before.client_sessions.keys().map(|client| (*client, None)))
        .collect::<BTreeMap<_, _>>()
    {
        let previous = before.client_sessions.get(&client).copied();
        let session = session.or_else(|| client_attached_session(inner, client));
        if previous == session {
            continue;
        }
        leading.extend(previous.and_then(|previous| session_active_pane(inner, previous)));
        leading.extend(session.and_then(|session| session_active_pane(inner, session)));
    }
    for (client, focused) in inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.focused.as_ref().map(|focused| (id, focused)))
    {
        if before.client_focused.get(client) != Some(focused) {
            leading.extend(client_active_pane(inner, *client));
        }
    }
    if gated {
        for (session, window) in &before.session_windows {
            let Some(current) = inner.engine.state.sessions.get(session) else {
                continue;
            };
            if current.active_window == *window {
                continue;
            }
            leading.extend(
                inner
                    .engine
                    .state
                    .windows
                    .get(window)
                    .map(|state| state.active_pane),
            );
            leading.extend(session_active_pane(inner, *session));
        }
    }
    for (window, pane) in &before.window_active_panes {
        let Some(current) = inner.engine.state.windows.get(window) else {
            continue;
        };
        if current.active_pane == *pane {
            continue;
        }
        if inner.engine.state.pane(*pane).is_some() {
            if gated {
                leading.push(*pane);
                leading.push(current.active_pane);
            }
        } else {
            removals.push((*window, current.active_pane));
        }
    }
    (leading, removals)
}

fn no_pane_can_hold_focus(inner: &ServerState) -> bool {
    inner.attachment.pane_focus.is_empty()
        && !inner
            .attachment
            .attached
            .values()
            .flatten()
            .any(|client| inner.client(*client).and_then(|c| c.focused.as_ref()) == Some(&true))
}

fn pane_focus_hook_events(inner: &mut ServerState, before: &PaneFocusProbe) -> PaneFocusEvents {
    inner
        .attachment
        .pane_focus
        .retain(|pane| inner.engine.state.pane(*pane).is_some());
    let (leading, removals) = pane_focus_candidates(inner, before);
    let mut events = PaneFocusEvents::default();
    for pane in leading {
        events.leading.extend(pane_focus_transition(inner, pane));
    }
    for (window, pane) in removals {
        events
            .after_window_pane_changed
            .extend(pane_focus_transition(inner, pane).map(|event| (window, event)));
    }
    events
}

fn splice_pane_focus_events(events: &mut Vec<PendingHookEvent>, at: usize, focus: PaneFocusEvents) {
    if focus.is_empty() {
        return;
    }
    let leading = focus.leading.len();
    events.splice(at..at, focus.leading);
    for (window, event) in focus.after_window_pane_changed {
        let anchor = events
            .iter()
            .rposition(|candidate| {
                candidate.name == "window-pane-changed"
                    && candidate.variables.get(HOOK_WINDOW_CONTEXT_FORMAT)
                        == Some(&window.to_string())
            })
            .map_or(at + leading, |index| index + 1);
        events.insert(anchor, event);
    }
}

fn client_focused_window_for_attachment(inner: &ServerState, client: ClientId) -> Option<WindowId> {
    client_attached_session(inner, client)
        .and_then(|session| inner.engine.state.sessions.get(&session))
        .map(|session| client_focused_window(inner, client, session))
}

fn client_hook_event_for_window(
    inner: &ServerState,
    name: &'static str,
    client: ClientId,
    window: WindowId,
) -> Option<PendingHookEvent> {
    let session = client_attached_session(inner, client)?;
    let window_state = inner.engine.state.windows.get(&window)?;
    if window_state.session != session {
        return None;
    }
    let pane = window_state.active_pane;
    let client_name = client_format_name(inner, client);
    Some(PendingHookEvent::client(
        name,
        ExecutionContext::new(Some(session), Some(window), Some(pane)),
        client,
        Some(client_name.as_str()),
    ))
}

fn client_hook_event(
    inner: &ServerState,
    name: &'static str,
    client: ClientId,
) -> Option<PendingHookEvent> {
    let window = client_focused_window_for_attachment(inner, client)?;
    client_hook_event_for_window(inner, name, client, window)
}

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

fn status_targets(
    inner: &ServerState,
    sessions: Option<&BTreeSet<SessionId>>,
    clients: Option<&BTreeSet<ClientId>>,
) -> Vec<ClientId> {
    inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.subscriber.as_ref().map(|_| id))
        .copied()
        .filter(|client| {
            inner
                .client(*client)
                .and_then(|c| c.ctrl_subscriptions.as_ref())
                .is_none_or(|subscription| subscription.status)
                && clients.is_none_or(|clients| clients.contains(client))
                && sessions.is_none_or(|sessions| {
                    client_attached_session(inner, *client)
                        .is_some_and(|session| sessions.contains(&session))
                })
        })
        .collect()
}

fn status_request_with_selected_options(
    inner: &ServerState,
    client: ClientId,
    mut option_snapshot: Option<Arc<zz_mux::StatusRowVariables>>,
    startup_ready: bool,
    job_needs: FormatNeeds,
) -> Arc<StatusRequest> {
    let revision = (zz_mux::borrowed_formats_enabled()
        && inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Control)
        && job_needs.is_empty()
        && inner
            .client(client)
            .is_none_or(|c| c.copy_session.is_none())
        && inner
            .client(client)
            .is_none_or(|c| c.command_output.is_none()))
    .then(|| inner.engine.format_cache_revision());
    let attached = client_attached_session(inner, client);
    let window = attached
        .and_then(|session| inner.engine.state.sessions.get(&session))
        .map(|session| client_focused_window(inner, client, session));
    let mut reuse_admission = false;
    if let Some(revision) = revision {
        let mut cache = inner.publication.status_preparation_cache.lock();
        let options = option_snapshot.get_or_insert_with(|| {
            cache
                .as_ref()
                .filter(|cached| {
                    cached.request.client == client
                        && (cached.revision.0, cached.revision.1, cached.revision.2)
                            == (revision.0, revision.1, revision.2)
                        && inner
                            .engine
                            .format_cache_identity_matches(&cached.engine_identity)
                })
                .map_or_else(
                    || inner.engine.cached_format_option_snapshot(),
                    |cached| Arc::clone(&cached.request.option_snapshot),
                )
        });
        let cached_matches = cache.as_ref().is_some_and(|cached| {
            cached.matches(
                inner,
                client,
                attached,
                window,
                revision,
                options,
                startup_ready,
            )
        });
        let context = if cached_matches {
            None
        } else {
            cache.as_ref().and_then(|cached| {
                reuse_admission = cached.matches(
                    inner,
                    client,
                    attached,
                    window,
                    (revision.0, revision.1, revision.2, cached.revision.3),
                    options,
                    startup_ready,
                );
                if !reuse_admission {
                    return None;
                }
                let parameters = status_parameters(inner, attached, options);
                cached_live_status_context(
                    inner,
                    attached,
                    window,
                    parameters.needs,
                    &cached.request.references,
                    cached.config_files_requested,
                )
                .filter(|context| context.same_detached_data(&cached.request.context))
                .map(Arc::new)
            })
        };
        if let Some(cached) = cache.as_ref()
            && (cached_matches || context.is_some())
        {
            let request = Arc::clone(&cached.request);
            let pane_borders = attached.map_or_else(
                || Arc::clone(&request.pane_borders),
                |session| {
                    cached_live_border_presentations(
                        inner,
                        client,
                        session,
                        window,
                        revision,
                        options,
                        || {
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map_or(0, |duration| duration.as_secs())
                        },
                    )
                    .unwrap_or_else(|| {
                        let facts = readonly_borrowed_format_hook_facts(
                            inner,
                            CommandFormatSeed {
                                client: cached
                                    .border_client
                                    .then(|| request.facts.client.clone())
                                    .flatten(),
                                invoking: Some(client),
                            },
                        );
                        border_presentations(inner, client, session, &facts)
                    })
                },
            );
            if context.is_none() && Arc::ptr_eq(&request.pane_borders, &pane_borders) {
                return request;
            }
            let request = Arc::new(StatusRequest {
                context: context.unwrap_or_else(|| Arc::clone(&request.context)),
                pane_borders,
                ..request.as_ref().clone()
            });
            let cached = cache.as_mut().unwrap();
            let same_storage = request.context.same_detached_data(&cached.request.context)
                && request.context.session_id.capacity()
                    == cached.request.context.session_id.capacity()
                && request.context.window_id.capacity()
                    == cached.request.context.window_id.capacity()
                && request.context.pane_id.capacity() == cached.request.context.pane_id.capacity()
                && Arc::ptr_eq(&request.pane_borders, &cached.request.pane_borders);
            cached.revision = revision;
            cached.request = Arc::clone(&request);
            if !same_storage {
                cached.retained_bytes = cached.retained_bytes();
            }
            if cached.retained_bytes > STATUS_PREPARATION_MAX_BYTES {
                *cache = None;
            }
            return request;
        }
    }
    let option_snapshot =
        option_snapshot.unwrap_or_else(|| inner.engine.cached_format_option_snapshot());
    let facts = (inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Control))
        .then(|| format_hook_facts(inner));
    let snapshot = facts.is_some().then(|| inner.engine.state.snapshot());
    let request = Arc::new(status_request_with_facts(
        inner,
        client,
        snapshot.as_ref(),
        option_snapshot,
        facts,
        startup_ready,
        job_needs,
        &mut BTreeMap::new(),
    ));
    if let Some(revision) = revision {
        let reusable = reuse_admission || {
            let parameters = status_parameters(inner, attached, &request.option_snapshot);
            let client_dependencies_safe = !parameters.client_fact_selection.full
                && parameters.client_references.iter().all(|name| {
                    (!name.starts_with("client_")
                        || matches!(name.as_str(), "client_width" | "client_colours"))
                        && !matches!(name.as_str(), "window_cell_height" | "window_cell_width")
                });
            crate::status::status_cache_callbacks(&request).is_some_and(|names| {
                names.iter().all(|name| {
                    matches!(
                        name.as_str(),
                        "client_width"
                            | "client_colours"
                            | "window_bigger"
                            | "window_offset_x"
                            | "window_offset_y"
                    )
                })
            }) && client_dependencies_safe
                && !request.references.iter().any(|name| name.starts_with('@'))
        };
        let cached = reusable.then(|| {
            CachedStatusPreparation::new(inner, attached, window, revision, request.clone())
        });
        *inner.publication.status_preparation_cache.lock() =
            cached.filter(|cached| cached.retained_bytes <= STATUS_PREPARATION_MAX_BYTES);
    }
    request
}

const STATUS_PREPARATION_MAX_BYTES: usize = 1024 * 1024;

struct CachedStatusPreparation {
    revision: (u64, u64, u64, u64),
    attached: Option<SessionId>,
    window: Option<WindowId>,
    size: Option<(u16, u16)>,
    kind: Option<ClientKind>,
    terminal: bool,
    features: Option<u32>,
    environment: Option<Weak<ClientEnvironmentBlob>>,
    config_files: String,
    request: Arc<StatusRequest>,
    viewport_requested: bool,
    border_client: bool,
    retained_bytes: usize,
    engine_identity: Weak<()>,
    config_files_requested: bool,
}

impl CachedStatusPreparation {
    fn new(
        inner: &ServerState,
        attached: Option<SessionId>,
        window: Option<WindowId>,
        revision: (u64, u64, u64, u64),
        request: Arc<StatusRequest>,
    ) -> Self {
        let client = request.client;
        let viewport_requested = status_parameters(inner, attached, &request.option_snapshot)
            .client_fact_selection
            .viewport;
        let border_references = inner
            .engine
            .cached_format_references_for_templates(BORDER_FORMAT_TEMPLATES);
        let border_client = border_references.iter().any(|name| {
            name.starts_with("client_")
                || matches!(
                    name.as_str(),
                    "window_bigger" | "window_offset_x" | "window_offset_y"
                )
        });
        let config_files_requested = !zz_mux::borrowed_formats_enabled()
            || !request.modes.is_empty()
            || request.references.contains("*")
            || request.references.contains("config_files")
            || border_references.contains("*")
            || border_references.contains("config_files");
        let mut cached = Self {
            revision,
            attached,
            window,
            size: inner.client(client).and_then(|c| c.size),
            kind: inner.client(client).and_then(|c| c.kind),
            terminal: inner.client(client).is_some_and(|c| c.has_terminal),
            features: inner.client(client).and_then(|c| c.features),
            environment: inner
                .client(client)
                .and_then(|c| c.environment.as_ref())
                .map(Arc::downgrade),
            config_files: if config_files_requested {
                inner.config.config_files.clone()
            } else {
                String::new()
            },
            request,
            viewport_requested,
            border_client,
            retained_bytes: 0,
            engine_identity: inner.engine.format_cache_identity(),
            config_files_requested,
        };
        cached.retained_bytes = cached.retained_bytes();
        cached
    }

    fn matches(
        &self,
        inner: &ServerState,
        client: ClientId,
        attached: Option<SessionId>,
        window: Option<WindowId>,
        revision: (u64, u64, u64, u64),
        options: &Arc<zz_mux::StatusRowVariables>,
        startup_ready: bool,
    ) -> bool {
        self.revision == revision
            && self.request.client == client
            && self.attached == attached
            && self.window == window
            && Arc::ptr_eq(&self.request.option_snapshot, options)
            && self.request.startup != startup_ready
            && self.request.client_scheme == inner.client(client).and_then(|c| c.color_scheme)
            && self.size == inner.client(client).and_then(|c| c.size)
            && self.kind == inner.client(client).and_then(|c| c.kind)
            && self.terminal == inner.client(client).is_some_and(|c| c.has_terminal)
            && self.features == inner.client(client).and_then(|c| c.features)
            && (!self.config_files_requested || self.config_files == inner.config.config_files)
            && (!self.viewport_requested
                || (self.size.is_some()
                    && self
                        .request
                        .facts
                        .client
                        .as_ref()
                        .and_then(|c| c.viewport)
                        .is_some_and(|viewport| !viewport.bigger()))
                || {
                    let current = (self.kind == Some(ClientKind::Interactive) && self.terminal)
                        .then(|| {
                            client_viewport_facts_from_source(
                                &ClientFormatSource::from_inner(inner),
                                client,
                                attached?,
                                window?,
                            )
                        })
                        .flatten();
                    let values =
                        |viewport: ClientViewportFacts| (viewport.bigger(), viewport.offsets());
                    current.map(values)
                        == self
                            .request
                            .facts
                            .client
                            .as_ref()
                            .and_then(|c| c.viewport)
                            .map(values)
                })
            && match (
                &self.environment,
                inner.client(client).and_then(|c| c.environment.as_ref()),
            ) {
                (None, None) => true,
                (Some(cached), Some(current)) => cached.as_ptr() == Arc::as_ptr(current),
                _ => false,
            }
    }

    fn retained_bytes(&self) -> usize {
        let environment = self.environment.as_ref().map_or(0, |_| {
            std::mem::size_of::<ClientEnvironmentBlob>()
                .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
        });
        let job_environment =
            self.request
                .environment
                .iter()
                .fold(0_usize, |bytes, (name, value)| {
                    bytes
                        .saturating_add(zz_mux::cloned_raw_text_bytes(name))
                        .saturating_add(value.as_ref().map_or(0, zz_mux::cloned_raw_text_bytes))
                        .saturating_add(128)
                });
        crate::status::completed_status_bytes(
            &self.request,
            &[],
            &[],
            &zz_protocol::StatusLine::default(),
        )
        .saturating_add(environment)
        .saturating_add(job_environment.saturating_mul(2))
        .saturating_add(self.config_files.capacity().saturating_mul(2))
        .saturating_add(self.request.default_terminal.capacity().saturating_mul(2))
        .saturating_add(std::mem::size_of::<Self>())
        .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
        .saturating_add(4096)
    }
}

#[cfg(test)]
mod format_preparation_tests;

fn status_request(
    inner: &ServerState,
    client: ClientId,
    snapshot: &MuxSnapshot,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
    facts: FormatHookFacts,
    startup_ready: bool,
    job_needs: FormatNeeds,
) -> StatusRequest {
    status_request_with(
        inner,
        client,
        snapshot,
        option_snapshot,
        facts,
        startup_ready,
        job_needs,
        &mut BTreeMap::new(),
    )
}

fn status_request_with(
    inner: &ServerState,
    client: ClientId,
    snapshot: &MuxSnapshot,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
    facts: FormatHookFacts,
    startup_ready: bool,
    job_needs: FormatNeeds,
    line_needs: &mut BTreeMap<Option<SessionId>, FormatNeeds>,
) -> StatusRequest {
    status_request_with_facts(
        inner,
        client,
        Some(snapshot),
        option_snapshot,
        Some(facts),
        startup_ready,
        job_needs,
        line_needs,
    )
}

struct StatusParameters {
    formats: Arc<zz_mux::StatusFormats>,
    row_formats: Arc<BTreeMap<u32, String>>,
    title_format: Arc<Option<String>>,
    environment: Arc<Vec<(RawText, Option<RawText>)>>,
    default_terminal: Arc<String>,
    message_styles: Arc<(String, String)>,
    message_line: u8,
    customized: bool,
    needs: FormatNeeds,
    references: Arc<BTreeSet<String>>,
    client_references: Arc<BTreeSet<String>>,
    fact_selection: StatusFactSelection,
    client_fact_selection: ClientFactSelection,
}

struct CachedStatusParameters {
    key: (Option<SessionId>, u64, u64, u64),
    parameters: Arc<StatusParameters>,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
}

impl StatusParameters {
    fn retained_bytes(&self) -> usize {
        let strings = [
            &self.formats.left,
            &self.formats.right,
            &self.formats.style,
            &self.formats.background,
            &self.formats.foreground,
            &self.formats.left_style,
            &self.formats.right_style,
            &self.message_styles.0,
            &self.message_styles.1,
            self.default_terminal.as_ref(),
        ];
        let text = strings
            .into_iter()
            .chain(self.row_formats.values())
            .chain(self.title_format.iter())
            .chain(self.references.iter())
            .chain(self.client_references.iter())
            .fold(0_usize, |bytes, value| {
                bytes.saturating_add(std::mem::size_of::<String>() + value.capacity())
            });
        let environment = self
            .environment
            .iter()
            .fold(0_usize, |bytes, (name, value)| {
                bytes
                    .saturating_add(std::mem::size_of::<(RawText, Option<RawText>)>())
                    .saturating_add(zz_mux::cloned_raw_text_bytes(name))
                    .saturating_add(value.as_ref().map_or(0, zz_mux::cloned_raw_text_bytes))
            });
        text.saturating_add(environment)
            .saturating_add(std::mem::size_of::<Self>())
            .saturating_mul(2)
            .saturating_add(4096)
    }
}

#[cfg(test)]
mod format_parameters_tests;

fn status_parameters(
    inner: &ServerState,
    attached: Option<SessionId>,
    option_snapshot: &Arc<zz_mux::StatusRowVariables>,
) -> Arc<StatusParameters> {
    let (state, options, data, _) = inner.engine.format_cache_revision();
    let key = (attached, state, options, data);
    if let Some(cached) = inner.publication.status_parameters_cache.lock().as_ref()
        && cached.key == key
        && Arc::ptr_eq(&cached.option_snapshot, option_snapshot)
    {
        return Arc::clone(&cached.parameters);
    }
    let formats = Arc::new(inner.engine.status_formats_for_session(attached));
    let row_formats = Arc::new(inner.engine.status_format_array_for_session(attached));
    let title_format = Arc::new(
        (attached.is_some() && inner.engine.set_titles_for_session(attached))
            .then(|| inner.engine.set_titles_string_for_session(attached)),
    );
    let message_styles = Arc::new(inner.engine.message_styles_for_session(attached));
    let needs = crate::status::status_line_needs(
        &inner.engine,
        &formats,
        &row_formats,
        title_format.as_deref(),
        &message_styles,
    );
    let (references, client_references) = {
        let references = inner.engine.cached_format_references_for_templates(
            crate::status::status_line_templates(
                &formats,
                &row_formats,
                title_format.as_deref(),
                &message_styles,
            ),
        );
        let border_references = inner.engine.cached_format_references_for_templates([
            "#{E:pane-border-style}",
            "#{E:pane-active-border-style}",
        ]);
        let client_references = Arc::new(
            references
                .iter()
                .chain(border_references.iter())
                .cloned()
                .collect(),
        );
        (references, client_references)
    };
    let fact_selection = StatusFactSelection::from_references(&references);
    let client_fact_selection = ClientFactSelection::from_references(&client_references);
    let parameters = Arc::new(StatusParameters {
        formats,
        row_formats,
        title_format,
        environment: Arc::new(inner.engine.job_environment(None)),
        default_terminal: Arc::new(inner.engine.default_terminal_for_spawn().to_owned()),
        message_styles,
        message_line: inner.engine.message_line_for_session(attached),
        customized: inner.engine.status_customized_for_session(attached),
        needs,
        references,
        client_references,
        fact_selection,
        client_fact_selection,
    });
    *inner.publication.status_parameters_cache.lock() = (parameters
        .retained_bytes()
        .saturating_add(option_snapshot.retained_bytes())
        <= 1024 * 1024)
        .then(|| CachedStatusParameters {
            key,
            parameters: Arc::clone(&parameters),
            option_snapshot: Arc::clone(option_snapshot),
        });
    parameters
}

fn cached_live_status_context(
    inner: &ServerState,
    attached: Option<SessionId>,
    focused_window: Option<WindowId>,
    needs: FormatNeeds,
    references: &Arc<BTreeSet<String>>,
    capture_config_files: bool,
) -> Option<zz_mux::StatusContext<'static>> {
    if references.contains("window_active_clients")
        || references.contains("window_active_clients_list")
    {
        return None;
    }
    let attached = attached.filter(|session| inner.engine.state.sessions.contains_key(session));
    let host = inner.engine.format_host().is_empty().then(host_names);
    let overrides = [
        capture_config_files.then_some(("config_files", inner.config.config_files.as_str())),
        host.map(|host| ("host", host.0.as_str())),
        host.map(|host| ("host_short", host.1.as_str())),
        attached.map(|_| ("pane_active", "1")),
        attached.map(|_| ("session_attached", "0")),
        attached.map(|_| ("session_attached_list", "")),
        attached.map(|_| ("session_many_attached", "0")),
        attached.map(|_| ("window_active", "1")),
        attached.map(|_| ("window_active_clients", "0")),
        attached.map(|_| ("window_active_clients_list", "")),
    ];
    inner
        .engine
        .cached_detached_format_context_with_shared_references(
            (attached, focused_window, None),
            attached.map_or(FormatClient::NoClient, FormatClient::Attached),
            needs,
            references,
            overrides.into_iter().flatten(),
        )
}

fn status_request_with_facts(
    inner: &ServerState,
    client: ClientId,
    snapshot: Option<&MuxSnapshot>,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
    facts: Option<FormatHookFacts>,
    startup_ready: bool,
    job_needs: FormatNeeds,
    _line_needs: &mut BTreeMap<Option<SessionId>, FormatNeeds>,
) -> StatusRequest {
    let attached = client_attached_session(inner, client);
    let focused_window = attached
        .and_then(|session| inner.engine.state.sessions.get(&session))
        .map(|session| client_focused_window(inner, client, session));
    let parameters = status_parameters(inner, attached, &option_snapshot);
    let needs = parameters.needs | job_needs;
    let references = if job_needs.is_empty() {
        Arc::clone(&parameters.references)
    } else {
        Arc::new(BTreeSet::from(["*".to_owned()]))
    };
    let modes = attached.map_or_else(Vec::new, |session| {
        mode_requests(inner, client, session, job_needs)
    });
    let capture_config_files = snapshot.is_some()
        || facts.is_some()
        || !modes.is_empty()
        || !job_needs.is_empty()
        || !zz_mux::borrowed_formats_enabled()
        || parameters.client_references.contains("*")
        || parameters.client_references.contains("config_files");
    let cached_context = snapshot
        .is_none()
        .then(|| {
            cached_live_status_context(
                inner,
                attached,
                focused_window,
                needs,
                &references,
                capture_config_files,
            )
        })
        .flatten();
    let live_context = cached_context.is_none().then(|| {
        let mut context = snapshot.map_or_else(
            || live_status_context(&inner.engine, attached, focused_window),
            |snapshot| status_context(snapshot, &inner.engine, attached, focused_window),
        );
        if capture_config_files {
            context.set_format_value("config_files", inner.config.config_files.clone());
        }
        context
    });
    let context = cached_context
        .as_ref()
        .or(live_context.as_ref())
        .expect("status context");
    let selected = facts.is_none();
    let mut facts = facts.unwrap_or_else(|| {
        if modes.is_empty() && job_needs.is_empty() {
            return selected_status_format_facts_with_selection(
                inner,
                context,
                &parameters.fact_selection,
            );
        }
        let mode_references;
        let fact_references = if modes.is_empty() {
            references.as_ref()
        } else {
            mode_references = references
                .iter()
                .cloned()
                .chain(
                    inner
                        .engine
                        .cached_format_references_for_templates(crate::status::MODE_FORMATS)
                        .iter()
                        .cloned(),
                )
                .collect::<BTreeSet<_>>();
            &mode_references
        };
        selected_status_format_facts(inner, context, fact_references)
    });
    if !selected || references.contains("*") || !modes.is_empty() {
        facts.client_environment = inner
            .client(client)
            .and_then(|c| c.environment.as_ref())
            .cloned();
    }
    if let Some(session) = attached {
        facts.client = Some(if selected && modes.is_empty() && job_needs.is_empty() {
            selected_client_format_facts_with_selection(
                inner,
                client,
                session,
                &parameters.client_fact_selection,
            )
        } else if selected && modes.is_empty() {
            selected_client_format_facts_with_references(inner, client, session, &references)
        } else {
            client_format_facts(inner, client, session)
        });
    }
    let pane_borders = if selected {
        let mut borrowed = readonly_borrowed_format_hook_facts(
            inner,
            CommandFormatSeed {
                client: facts.client.take(),
                invoking: Some(client),
            },
        );
        let borders = attached.map_or_else(Arc::default, |session| {
            border_presentations(inner, client, session, &borrowed)
        });
        facts.client = borrowed.seed.client.take();
        borders
    } else {
        attached.map_or_else(Arc::default, |session| {
            border_presentations(inner, client, session, &facts)
        })
    };
    let context = cached_context.unwrap_or_else(|| {
        live_context
            .expect("uncached status context")
            .detach_with_shared_references(needs, &references)
    });
    StatusRequest {
        client,
        formats: Arc::clone(&parameters.formats),
        row_formats: Arc::clone(&parameters.row_formats),
        option_snapshot,
        message_line: parameters.message_line,
        customized: parameters.customized,
        title_format: Arc::clone(&parameters.title_format),
        environment: Arc::clone(&parameters.environment),
        default_terminal: Arc::clone(&parameters.default_terminal),
        startup: !startup_ready,
        context: Arc::new(context),
        facts: Arc::new(facts),
        client_scheme: inner.client(client).and_then(|c| c.color_scheme),
        message_styles: Arc::clone(&parameters.message_styles),
        modes,
        pane_borders,
        references,
    }
}

const BORDER_FORMAT_CACHE_BYTES: usize = 1024 * 1024;
const BORDER_FORMAT_TEMPLATES: [&str; 4] = [
    "#{E:pane-border-style}",
    "#{E:pane-active-border-style}",
    "#{E:window-style}",
    "#{E:window-active-style}",
];

struct CachedBorderPresentations {
    revision: (u64, u64, u64, u64),
    second: u64,
    client: ClientId,
    session: SessionId,
    window: WindowId,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
    references: Arc<BTreeSet<String>>,
    callbacks: Vec<String>,
    panes: Vec<CachedBorderPane>,
    presentations: Arc<Vec<zz_protocol::PaneBorderPresentation>>,
    clock_dependent: bool,
}

struct CachedBorderPane {
    context: zz_mux::StatusContext<'static>,
    callback_values: Vec<Option<String>>,
    presentation: zz_protocol::PaneBorderPresentation,
    pane_in_mode: Option<usize>,
}

impl CachedBorderPresentations {
    fn matches(
        &self,
        revision: (u64, u64, u64, u64),
        second: u64,
        target: (ClientId, SessionId, WindowId),
        option_snapshot: &Arc<zz_mux::StatusRowVariables>,
    ) -> bool {
        (self.revision.0, self.revision.1, self.revision.2) == (revision.0, revision.1, revision.2)
            && (!self.clock_dependent || (self.revision.3 == revision.3 && self.second == second))
            && (self.client, self.session, self.window) == target
            && Arc::ptr_eq(&self.option_snapshot, option_snapshot)
    }

    fn same_callbacks(&self, facts: &dyn crate::status::FormatFactSource) -> bool {
        if !self.panes.iter().all(|pane| {
            pane.pane_in_mode
                .is_none_or(|count| facts.pane_in_mode_count(pane.presentation.pane) == count)
        }) {
            return false;
        }
        if self.callbacks.is_empty() {
            return true;
        }
        let mut hooks = DaemonFormatHooks::command(facts);
        self.panes.iter().all(|pane| {
            self.callbacks
                .iter()
                .zip(&pane.callback_values)
                .all(|(name, expected)| hooks.variable(name, &pane.context) == *expected)
        })
    }

    fn presentations(&self) -> Arc<Vec<zz_protocol::PaneBorderPresentation>> {
        Arc::clone(&self.presentations)
    }

    fn retained_bytes(&self) -> usize {
        let mut bytes =
            std::mem::size_of::<Self>().saturating_add(self.option_snapshot.retained_bytes());
        for reference in self.references.iter().chain(&self.callbacks) {
            bytes = bytes
                .saturating_add(std::mem::size_of::<String>())
                .saturating_add(reference.capacity());
        }
        bytes = bytes.saturating_add(
            self.panes
                .capacity()
                .saturating_mul(std::mem::size_of::<CachedBorderPane>()),
        );
        for pane in &self.panes {
            bytes = bytes
                .saturating_add(pane.context.retained_bytes())
                .saturating_add(pane.presentation.style.capacity())
                .saturating_add(pane.presentation.window_style.capacity())
                .saturating_add(pane.presentation.window_active_style.capacity())
                .saturating_add(
                    pane.callback_values
                        .capacity()
                        .saturating_mul(std::mem::size_of::<Option<String>>()),
                );
            for value in pane.callback_values.iter().flatten() {
                bytes = bytes.saturating_add(value.capacity());
            }
        }
        bytes = bytes.saturating_add(
            self.presentations
                .capacity()
                .saturating_mul(std::mem::size_of::<zz_protocol::PaneBorderPresentation>()),
        );
        for presentation in self.presentations.iter() {
            bytes = bytes.saturating_add(presentation.style.capacity());
        }
        bytes.saturating_mul(2).saturating_add(4096)
    }
}

fn border_format_callbacks(
    engine: &MuxEngine,
    context: &zz_mux::StatusContext,
    option_snapshot: &zz_mux::StatusRowVariables,
    references: &BTreeSet<String>,
) -> Option<Vec<String>> {
    let needs = engine.cached_format_needs(BORDER_FORMAT_TEMPLATES);
    if references.contains("*")
        || [
            FormatNeeds::SESSIONS,
            FormatNeeds::WINDOWS,
            FormatNeeds::PANES,
            FormatNeeds::OPTIONS,
        ]
        .iter()
        .any(|need| needs.contains(*need))
    {
        return None;
    }
    let mut callbacks = Vec::new();
    for name in references.iter().map(String::as_str) {
        if matches!(name, "window_active_clients" | "window_active_clients_list") {
            return None;
        }
        let factual = (name.starts_with("client_")
            && zz_mux::delegated_format_variable_names().any(|known| known == name))
            || matches!(
                name,
                "pane_in_mode"
                    | "pane_mode"
                    | "pane_unseen_changes"
                    | "pane_pipe"
                    | "pane_pipe_pid"
                    | "session_last_attached"
                    | "session_attached"
                    | "session_attached_list"
                    | "session_many_attached"
                    | "window_bigger"
                    | "window_offset_x"
                    | "window_offset_y"
                    | "window_cell_height"
                    | "window_cell_width"
            );
        if factual {
            callbacks.push(name.to_owned());
            continue;
        }
        let option_name = name
            .strip_prefix("E:")
            .or_else(|| name.strip_prefix("T:"))
            .unwrap_or(name);
        if name != "config_files"
            && (zz_mux::format_variable_is_captured(name)
                || option_snapshot
                    .lookup(
                        &context.session_id,
                        &context.window_id,
                        &context.pane_id,
                        option_name,
                    )
                    .is_some()
                || (option_name.starts_with('@')
                    && engine
                        .format_user_option(
                            &context.pane_id,
                            &context.window_id,
                            &context.session_id,
                            option_name,
                        )
                        .is_some()))
        {
            continue;
        }
        return None;
    }
    Some(callbacks)
}

fn border_format_clock_dependent(
    engine: &MuxEngine,
    context: &zz_mux::StatusContext,
    option_snapshot: &zz_mux::StatusRowVariables,
    references: &BTreeSet<String>,
) -> bool {
    BORDER_FORMAT_TEMPLATES
        .into_iter()
        .any(zz_mux::format_clock_dependent)
        || references.iter().any(|name| {
            if name == "*" || name.starts_with("T:") {
                return true;
            }
            let Some(name) = name.strip_prefix("E:") else {
                return false;
            };
            if let Some(source) = option_snapshot.lookup(
                &context.session_id,
                &context.window_id,
                &context.pane_id,
                name,
            ) {
                return zz_mux::format_clock_dependent(&source);
            }
            engine
                .format_user_option(
                    &context.pane_id,
                    &context.window_id,
                    &context.session_id,
                    name,
                )
                .is_none_or(zz_mux::format_clock_dependent)
        })
}

#[cfg(test)]
thread_local! {
    static BORDER_FORMAT_EXPANSIONS: Cell<usize> = const { Cell::new(0) };
}

fn border_format_style(
    format: &str,
    context: &zz_mux::StatusContext,
    hooks: &mut DaemonFormatHooks<'_>,
) -> String {
    #[cfg(test)]
    BORDER_FORMAT_EXPANSIONS.with(|count| count.set(count.get() + 1));
    crate::status::expand_style(format, context, hooks)
}

fn window_style_presentation(
    engine: &MuxEngine,
    active: bool,
    context: &zz_mux::StatusContext,
    hooks: &mut DaemonFormatHooks<'_>,
) -> (String, String) {
    if !engine.has_window_style_settings() {
        return (String::new(), String::new());
    }
    let window_style = crate::status::expand_style(BORDER_FORMAT_TEMPLATES[2], context, hooks);
    let window_active_style = if active {
        crate::status::expand_style(BORDER_FORMAT_TEMPLATES[3], context, hooks)
    } else {
        String::new()
    };
    (window_style, window_active_style)
}

#[cfg(test)]
#[path = "daemon/format_border_tests.rs"]
mod format_border_tests;

fn border_presentations(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    facts: &dyn crate::status::FormatFactSource,
) -> Arc<Vec<zz_protocol::PaneBorderPresentation>> {
    let second = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    border_presentations_at(inner, client, session, facts, second)
}

fn cached_live_border_presentations(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    window: Option<WindowId>,
    revision: (u64, u64, u64, u64),
    option_snapshot: &Arc<zz_mux::StatusRowVariables>,
    second: impl FnOnce() -> u64,
) -> Option<Arc<Vec<zz_protocol::PaneBorderPresentation>>> {
    let window = window?;
    inner.engine.state.windows.get(&window)?;
    let cache = inner.publication.border_presentations_cache.lock();
    let cached = cache.as_ref()?;
    let second = if cached.clock_dependent {
        second()
    } else {
        cached.second
    };
    (cached.callbacks.is_empty()
        && cached.matches(revision, second, (client, session, window), option_snapshot)
        && cached.panes.iter().all(|pane| {
            pane.pane_in_mode.is_none_or(|count| {
                pane_in_mode_count_from_parts(
                    pane.presentation.pane,
                    &inner.modes.pane_modes,
                    &inner.clients,
                    &inner.terminals,
                ) == count
            })
        }))
    .then(|| cached.presentations())
}

fn border_presentations_at(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    facts: &dyn crate::status::FormatFactSource,
    second: u64,
) -> Arc<Vec<zz_protocol::PaneBorderPresentation>> {
    let revision = inner.engine.format_cache_revision();
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return Arc::default();
    };
    let window = client_focused_window(inner, client, session_state);
    let Some(window_state) = inner.engine.state.windows.get(&window) else {
        return Arc::default();
    };
    let option_snapshot = inner.engine.cached_format_option_snapshot();
    {
        let cache = inner.publication.border_presentations_cache.lock();
        if let Some(cached) = cache.as_ref()
            && cached.matches(
                revision,
                second,
                (client, session, window),
                &option_snapshot,
            )
            && cached.same_callbacks(facts)
        {
            return cached.presentations();
        }
    }
    let references = inner
        .engine
        .cached_format_references_for_templates(BORDER_FORMAT_TEMPLATES);
    let contexts = inner
        .engine
        .format_context_snapshot(FormatClient::Attached(session));
    let first_context =
        contexts.status_context(Some(session), Some(window), Some(window_state.active_pane));
    let Some(mut callbacks) =
        border_format_callbacks(&inner.engine, &first_context, &option_snapshot, &references)
    else {
        *inner.publication.border_presentations_cache.lock() = None;
        return uncached_border_presentations(inner, client, session, facts);
    };
    let pane_in_mode = callbacks.iter().any(|name| name == "pane_in_mode");
    callbacks.retain(|name| name != "pane_in_mode");
    let mut hooks = DaemonFormatHooks::command(facts).with_option_engine(&inner.engine);
    let mut clock_dependent = false;
    let panes: Vec<CachedBorderPane> = window_state
        .panes
        .keys()
        .take(zz_protocol::MAX_PANE_BORDER_PRESENTATIONS)
        .map(|pane| {
            let context = contexts.status_context(Some(session), Some(window), Some(*pane));
            clock_dependent |= border_format_clock_dependent(
                &inner.engine,
                &context,
                &option_snapshot,
                &references,
            );
            let pane_in_mode = pane_in_mode.then(|| facts.pane_in_mode_count(*pane));
            if let Some(count) = pane_in_mode {
                hooks.set_pane_in_mode_count(*pane, count);
            }
            let active = *pane == window_state.active_pane;
            let format = BORDER_FORMAT_TEMPLATES[usize::from(active)];
            let style = border_format_style(format, &context, &mut hooks);
            let (window_style, window_active_style) =
                window_style_presentation(&inner.engine, active, &context, &mut hooks);
            let presentation = zz_protocol::PaneBorderPresentation {
                pane: *pane,
                style,
                window_style,
                window_active_style,
            };
            let mut callback_context = zz_mux::StatusContext::default();
            callback_context.session_id.clone_from(&context.session_id);
            callback_context.window_id.clone_from(&context.window_id);
            callback_context.pane_id.clone_from(&context.pane_id);
            let callback_values = callbacks
                .iter()
                .map(|name| hooks.variable(name, &context))
                .collect();
            CachedBorderPane {
                context: callback_context,
                callback_values,
                presentation,
                pane_in_mode,
            }
        })
        .collect();
    let presentations = Arc::new(panes.iter().map(|pane| pane.presentation.clone()).collect());
    let cached = CachedBorderPresentations {
        revision,
        second,
        client,
        session,
        window,
        option_snapshot,
        references,
        callbacks,
        panes,
        presentations,
        clock_dependent,
    };
    let result = cached.presentations();
    *inner.publication.border_presentations_cache.lock() =
        (cached.retained_bytes() <= BORDER_FORMAT_CACHE_BYTES).then_some(cached);
    result
}

fn uncached_border_presentations(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    facts: &dyn crate::status::FormatFactSource,
) -> Arc<Vec<zz_protocol::PaneBorderPresentation>> {
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return Arc::default();
    };
    let window = client_focused_window(inner, client, session_state);
    let Some(window_state) = inner.engine.state.windows.get(&window) else {
        return Arc::default();
    };
    let mut hooks = DaemonFormatHooks::command(facts).with_option_engine(&inner.engine);
    let contexts = inner
        .engine
        .format_context_snapshot(FormatClient::Attached(session));
    Arc::new(
        window_state
            .panes
            .keys()
            .take(zz_protocol::MAX_PANE_BORDER_PRESENTATIONS)
            .map(|pane| {
                let context = contexts.status_context(Some(session), Some(window), Some(*pane));
                let active = *pane == window_state.active_pane;
                let format = BORDER_FORMAT_TEMPLATES[usize::from(active)];
                let style = border_format_style(format, &context, &mut hooks);
                let (window_style, window_active_style) =
                    window_style_presentation(&inner.engine, active, &context, &mut hooks);
                zz_protocol::PaneBorderPresentation {
                    pane: *pane,
                    style,
                    window_style,
                    window_active_style,
                }
            })
            .collect(),
    )
}

fn mode_requests(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    job_needs: FormatNeeds,
) -> Vec<crate::status::ModeRequest> {
    let copy = inner
        .client(client)
        .and_then(|c| c.copy_session.as_ref())
        .filter(|copy| !copy.exiting)
        .and_then(|copy| Some((copy.pane, inner.terminals.get(&copy.pane)?, false)));
    let view = inner
        .client(client)
        .and_then(|c| c.command_output.as_ref())
        .map(|output| (output.pane, &output.terminal, true));
    let mut shown = copy
        .into_iter()
        .chain(view)
        .filter_map(|(pane, terminal, view)| {
            mode_request_position(client, terminal, view).map(|position| (pane, view, position))
        })
        .peekable();
    if shown.peek().is_none() {
        return Vec::new();
    }
    let needs = inner
        .engine
        .cached_format_needs(crate::status::MODE_FORMATS)
        | job_needs;
    let contexts = inner
        .engine
        .format_context_snapshot(FormatClient::Attached(session));
    shown
        .filter_map(|(pane, view, position)| {
            mode_request(
                inner,
                &contexts,
                needs,
                !job_needs.is_empty(),
                session,
                pane,
                view,
                position,
            )
        })
        .collect()
}

fn mode_request(
    inner: &ServerState,
    contexts: &zz_mux::FormatContextSnapshot<'_>,
    needs: FormatNeeds,
    dynamic_jobs: bool,
    session: SessionId,
    pane: PaneId,
    view: bool,
    shown: ModeShown,
) -> Option<crate::status::ModeRequest> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let context = contexts.status_context(Some(session), Some(window), Some(pane));
    let context = if dynamic_jobs {
        context.detach(needs)
    } else {
        context.detach_with_templates(needs, crate::status::MODE_FORMATS)
    };
    Some(crate::status::ModeRequest {
        pane,
        view,
        context,
        position: shown.position,
        limit: shown.limit,
        line_numbers: shown.line_numbers,
        hide_position: shown.hide_position,
        rows: shown.rows,
        vi_keys: inner
            .engine
            .copy_mode_table_for_pane(pane)
            .is_ok_and(|table| table == "copy-mode-vi"),
    })
}

const fn mode_kind(mode: TerminalMode) -> u8 {
    match mode {
        TerminalMode::Live => 0,
        TerminalMode::Copy {
            hide_position: false,
            ..
        } => 1,
        TerminalMode::Copy {
            hide_position: true,
            ..
        } => 2,
        TerminalMode::View { .. } => 3,
    }
}

#[derive(Clone, Copy)]
struct ModeShown {
    position: u32,
    limit: u32,
    line_numbers: u8,
    hide_position: bool,
    rows: u32,
}

fn mode_request_position(
    client: ClientId,
    terminal: &TerminalSession,
    view: bool,
) -> Option<ModeShown> {
    let viewport = terminal.latest_viewport_for(TerminalViewId(client.0))?;
    let line_numbers = if view {
        0
    } else {
        terminal
            .copy_mode_facts(TerminalViewId(client.0))
            .map_or(0, |facts| facts.line_numbers)
    };
    let (shown, hide_position) = match viewport.mode {
        TerminalMode::Copy { hide_position, .. } => (
            !view && (!hide_position || line_numbers != 0),
            hide_position,
        ),
        TerminalMode::View { .. } => (view, false),
        TerminalMode::Live => (false, false),
    };
    if !shown {
        return None;
    }
    let limit = viewport
        .scrollbar
        .total
        .saturating_sub(viewport.scrollbar.len);
    Some(ModeShown {
        position: limit.saturating_sub(viewport.scrollbar.offset),
        limit,
        line_numbers,
        hide_position,
        rows: viewport.scrollbar.len,
    })
}

fn resolve_popup_client(
    inner: &ServerState,
    invoking_client: ClientId,
    context: &ExecutionContext,
    target: Option<&str>,
) -> Result<ClientId, ServerError> {
    let eligible = inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.subscriber.as_ref().map(|_| id))
        .copied()
        .filter(|client| client_attached_session(inner, *client).is_some())
        .collect::<Vec<_>>();
    if let Some(target) = target {
        return find_attached_client_with_aliases(inner, target, true)
            .ok_or_else(|| client_target_error(target));
    }
    if eligible.contains(&invoking_client) {
        return Ok(invoking_client);
    }
    if let Some(owner) = context.pane.and_then(|pane| {
        terminal_geometry_owner(inner, pane).or_else(|| {
            let window = inner.engine.state.window_for_pane(pane)?;
            window_geometry_owner(inner, inner.engine.state.windows.get(&window)?)
                .map(|(_, owner)| owner)
        })
    }) && eligible.contains(&owner)
    {
        return Ok(owner);
    }
    eligible
        .into_iter()
        .next()
        .ok_or_else(|| ServerError::InvalidCommand("no current client".to_owned()))
}

const POPUP_CENTRE_X_CONTEXT_FORMAT: &str = "popup_centre_x";
const POPUP_CENTRE_Y_CONTEXT_FORMAT: &str = "popup_centre_y";
const POPUP_HEIGHT_CONTEXT_FORMAT: &str = "popup_height";
const POPUP_LAST_X_CONTEXT_FORMAT: &str = "popup_last_x";
const POPUP_LAST_Y_CONTEXT_FORMAT: &str = "popup_last_y";
const POPUP_MOUSE_BOTTOM_CONTEXT_FORMAT: &str = "popup_mouse_bottom";
const POPUP_MOUSE_CENTRE_X_CONTEXT_FORMAT: &str = "popup_mouse_centre_x";
const POPUP_MOUSE_CENTRE_Y_CONTEXT_FORMAT: &str = "popup_mouse_centre_y";
const POPUP_MOUSE_TOP_CONTEXT_FORMAT: &str = "popup_mouse_top";
const POPUP_MOUSE_X_CONTEXT_FORMAT: &str = "popup_mouse_x";
const POPUP_MOUSE_Y_CONTEXT_FORMAT: &str = "popup_mouse_y";
const POPUP_PANE_BOTTOM_CONTEXT_FORMAT: &str = "popup_pane_bottom";
const POPUP_PANE_LEFT_CONTEXT_FORMAT: &str = "popup_pane_left";
const POPUP_PANE_RIGHT_CONTEXT_FORMAT: &str = "popup_pane_right";
const POPUP_PANE_TOP_CONTEXT_FORMAT: &str = "popup_pane_top";
const POPUP_STATUS_LINE_Y_CONTEXT_FORMAT: &str = "popup_status_line_y";
const POPUP_WIDTH_CONTEXT_FORMAT: &str = "popup_width";
const POPUP_WINDOW_STATUS_LINE_X_CONTEXT_FORMAT: &str = "popup_window_status_line_x";
const POPUP_WINDOW_STATUS_LINE_Y_CONTEXT_FORMAT: &str = "popup_window_status_line_y";
#[cfg(test)]
const POPUP_POSITION_CONTEXT_FORMATS: [&str; 19] = [
    POPUP_CENTRE_X_CONTEXT_FORMAT,
    POPUP_CENTRE_Y_CONTEXT_FORMAT,
    POPUP_HEIGHT_CONTEXT_FORMAT,
    POPUP_LAST_X_CONTEXT_FORMAT,
    POPUP_LAST_Y_CONTEXT_FORMAT,
    POPUP_MOUSE_BOTTOM_CONTEXT_FORMAT,
    POPUP_MOUSE_CENTRE_X_CONTEXT_FORMAT,
    POPUP_MOUSE_CENTRE_Y_CONTEXT_FORMAT,
    POPUP_MOUSE_TOP_CONTEXT_FORMAT,
    POPUP_MOUSE_X_CONTEXT_FORMAT,
    POPUP_MOUSE_Y_CONTEXT_FORMAT,
    POPUP_PANE_BOTTOM_CONTEXT_FORMAT,
    POPUP_PANE_LEFT_CONTEXT_FORMAT,
    POPUP_PANE_RIGHT_CONTEXT_FORMAT,
    POPUP_PANE_TOP_CONTEXT_FORMAT,
    POPUP_STATUS_LINE_Y_CONTEXT_FORMAT,
    POPUP_WIDTH_CONTEXT_FORMAT,
    POPUP_WINDOW_STATUS_LINE_X_CONTEXT_FORMAT,
    POPUP_WINDOW_STATUS_LINE_Y_CONTEXT_FORMAT,
];

type SnapshotPresence = BTreeMap<SessionId, Vec<(ClientId, SessionViewer)>>;

fn snapshot_presence(inner: &ServerState) -> SnapshotPresence {
    inner
        .attachment
        .attached
        .iter()
        .filter_map(|(session, clients)| {
            let session_state = inner.engine.state.sessions.get(session)?;
            let viewers = clients
                .iter()
                .map(|client| {
                    let name = if inner.client(*client).and_then(|c| c.kind)
                        == Some(ClientKind::Control)
                    {
                        client_format_name(inner, *client)
                    } else {
                        inner
                            .client(*client)
                            .and_then(|c| c.name.as_ref())
                            .filter(|name| !name.is_empty())
                            .cloned()
                            .unwrap_or_else(|| format!("device-{}", client.0))
                    };
                    (
                        *client,
                        SessionViewer {
                            name,
                            window: client_focused_window(inner, *client, session_state),
                            is_self: false,
                        },
                    )
                })
                .collect();
            Some((*session, viewers))
        })
        .collect()
}

#[cfg(unix)]
mod hook_queue;
mod timers;

#[cfg(all(test, unix))]
mod publish_tests;

fn simulator_panes(inner: &ServerState) -> BTreeMap<PaneId, SimulatorDescriptor> {
    inner
        .engine
        .state
        .windows
        .values()
        .flat_map(|window| window.panes.values())
        .filter_map(|pane| match &pane.kind {
            PaneKind::Simulator(descriptor) => Some((pane.id, descriptor.clone())),
            _ => None,
        })
        .collect()
}

#[cfg(feature = "agent")]
fn record_agent_parents(
    inner: &mut ServerState,
    client: ClientId,
    context: &ExecutionContext,
    command_name: &str,
    effects: &[MuxEffect],
) -> Vec<PaneId> {
    let caller = inner
        .client(context.replay_client().unwrap_or(client))
        .and_then(|client| client.origin)
        .filter(|origin| is_agent_chat(inner, *origin));
    let opens = matches!(command_name, "new-pane" | "split-window" | "new-window");
    let mut first_chat = true;
    let mut children = Vec::new();
    for effect in effects {
        let MuxEffect::PaneCreated { pane, kind, .. } = effect else {
            continue;
        };
        let opened = matches!(kind, PaneKindSnapshot::Agent(_)) && std::mem::take(&mut first_chat);
        let reopened = context.agent_parent().filter(|_| opened);
        let parent = reopened.or_else(|| {
            caller.filter(|caller| {
                caller != pane
                    && ((opened && opens) || inner.engine.agent_parent(*caller).is_some())
            })
        });
        if let Some(parent) = parent {
            inner.engine.set_agent_parent(*pane, parent);
            children.push(*pane);
        }
    }
    publish_agent_parents(inner);
    children
}

fn publish_agent_parents(inner: &mut ServerState) {
    if *inner.engine.agent_parents() == inner.agents.published_agent_parents {
        return;
    }
    inner.agents.published_agent_parents = inner.engine.agent_parents().clone();
    let message = Shared::event(EventPayload::AgentParents {
        parents: inner.agents.published_agent_parents.clone(),
    });
    let Ok(encoded) = zz_protocol::encode_protocol_message(&message).map(Arc::<[u8]>::from) else {
        return;
    };
    for subscriber in inner
        .clients
        .values()
        .filter_map(|client| client.subscriber.as_ref())
    {
        let _ = subscriber.enqueue_reliable_frame(&message, Arc::clone(&encoded).into(), true);
    }
}

fn send_agent_parents(inner: &ServerState, outbound: &OutboundMailbox) {
    let parents = inner.engine.agent_parents();
    if !parents.is_empty() {
        Shared::send_event(
            outbound,
            EventPayload::AgentParents {
                parents: parents.clone(),
            },
        );
    }
}

fn stamp_snapshot_for_client(
    inner: &ServerState,
    client: ClientId,
    snapshot: &mut MuxSnapshot,
    presence: &SnapshotPresence,
) {
    let facts = format_hook_facts(inner);
    stamp_snapshot_for_client_with(inner, client, snapshot, presence, &facts);
}

fn stamp_snapshot_for_client_with(
    inner: &ServerState,
    client: ClientId,
    snapshot: &mut MuxSnapshot,
    presence: &SnapshotPresence,
    facts: &FormatHookFacts,
) {
    snapshot.focused_window = client_focused_window_for_attachment(inner, client);
    for session in &mut snapshot.sessions {
        session.viewers = presence
            .get(&session.id)
            .into_iter()
            .flatten()
            .map(|(viewer_client, viewer)| SessionViewer {
                is_self: *viewer_client == client,
                ..viewer.clone()
            })
            .collect();
    }
    let format_client = client_attached_session(inner, client)
        .map_or(FormatClient::Unattached, FormatClient::Attached);
    let contexts = inner.engine.format_context_snapshot(format_client);
    expand_window_status_labels(
        &inner.engine,
        &inner.config.config_files,
        facts,
        &contexts,
        snapshot,
    );
    stamp_pane_border_colours(
        &inner.engine,
        &inner.config.config_files,
        facts,
        &contexts,
        snapshot,
    );
    let border_cycle = stamp_pane_border_chrome(
        &inner.engine,
        &inner.config.config_files,
        facts,
        &contexts,
        snapshot,
    );
    let mut cycling = inner.attachment.border_cycle.lock();
    if border_cycle {
        cycling.insert(client);
    } else {
        cycling.remove(&client);
    }
    drop(cycling);
    drop(contexts);
    stamp_pane_modes(inner, facts, snapshot);
}

fn stamp_pane_modes(inner: &ServerState, facts: &FormatHookFacts, snapshot: &mut MuxSnapshot) {
    if inner.modes.pane_modes.is_empty() {
        return;
    }
    let engine = &inner.engine;
    for session in &mut snapshot.sessions {
        for window in &mut session.windows {
            for (pane, pane_snapshot) in &mut window.panes {
                pane_snapshot.mode = inner
                    .modes
                    .pane_modes
                    .get(pane)
                    .and_then(|modes| modes.last())
                    .map(|mode| match mode {
                        PaneModeRequest::Clock => {
                            let (colour, style) = engine.clock_mode_options(window.id);
                            PaneMode::Clock {
                                time: clock_mode_time(facts, style),
                                colour,
                            }
                        }
                        PaneModeRequest::Customize(mode) => {
                            let mut expand = customize_expander(inner, *pane, facts);
                            let (state, mut presentation, prompt) =
                                engine.customize_presentation(*pane, mode, &mut expand);
                            presentation.selection_style =
                                chooser_presentation::tree_selection_style_for_pane(inner, *pane);
                            presentation.border_style =
                                chooser_presentation::border_style_for_pane(inner, *pane);
                            let (prompt_type, prompt_flags) = mode.prompt_kind();
                            (presentation.prompt_style, presentation.prompt_cursor) =
                                chooser_presentation::mode_prompt_look(
                                    inner,
                                    chooser_presentation::pane_session(inner, *pane),
                                    prompt_type,
                                    prompt_flags,
                                    &mode.prompt_input(),
                                    mode.prompt_command_mode(),
                                );
                            let (prompt, prompt_cursor, prompt_top) = prompt.unwrap_or_default();
                            PaneMode::Customize {
                                state,
                                presentation: Box::new(presentation),
                                offset: MuxEngine::customize_offset(mode),
                                prompt,
                                prompt_cursor,
                                prompt_top,
                            }
                        }
                        PaneModeRequest::Panes(mode) => {
                            panes_mode_snapshot(inner, facts, *pane, mode)
                        }
                        PaneModeRequest::Switch(mode) => {
                            let entries = chooser_presentation::switch_matches(inner, *pane, mode);
                            let (columns, rows) = engine.pane_geometry(*pane).unwrap_or((80, 24));
                            let visible = usize::from(rows).saturating_sub(1);
                            let mut current = mode.current;
                            let mut offset = mode.offset;
                            if entries.is_empty() {
                                current = 0;
                                offset = 0;
                            } else {
                                current = current.min(entries.len() - 1);
                                if current < offset {
                                    offset = current;
                                } else if visible != 0 && current >= offset + visible {
                                    offset = current + 1 - visible;
                                }
                            }
                            let (prompt, prompt_cursor) = mode.prompt.draw(columns);
                            let (prompt_style, prompt_shape) =
                                chooser_presentation::mode_prompt_look(
                                    inner,
                                    chooser_presentation::pane_session(inner, *pane),
                                    "search",
                                    &["INCREMENTAL", "NOFORMAT"],
                                    &mode.prompt.input(),
                                    mode.prompt.command_mode(),
                                );
                            let matches = entries
                                .iter()
                                .map(|entry| {
                                    entry
                                        .columns
                                        .iter()
                                        .filter_map(|column| u16::try_from(*column).ok())
                                        .collect()
                                })
                                .collect();
                            PaneMode::Switch {
                                rows: entries.into_iter().map(|entry| entry.text).collect(),
                                selected: u32::try_from(current).unwrap_or(u32::MAX),
                                offset: u32::try_from(offset).unwrap_or(u32::MAX),
                                selection_style: chooser_presentation::mode_style_for_pane(
                                    inner, *pane,
                                ),
                                prompt,
                                prompt_style,
                                prompt_cursor,
                                matches,
                                match_style: chooser_presentation::switch_match_style(inner, *pane),
                                prompt_shape,
                            }
                        }
                    });
            }
        }
    }
}

fn panes_mode_snapshot(
    inner: &ServerState,
    facts: &FormatHookFacts,
    pane: PaneId,
    mode: &PanesMode,
) -> PaneMode {
    let engine = &inner.engine;
    let (columns, rows) = engine.pane_geometry(pane).unwrap_or((80, 24));
    let options = engine.display_panes_options(
        engine
            .state
            .window_for_pane(pane)
            .unwrap_or(mode.source_window),
    );
    let geometry = engine
        .panes_mode_geometry(mode.source_window, columns, rows)
        .unwrap_or_default();
    let active = engine
        .state
        .windows
        .get(&mode.source_window)
        .map(|window| window.active_pane);
    let captured = inner.modes.panes_mode_frames.get(&pane);
    let resized = captured.is_none_or(|captured| {
        if captured.size != Some((columns, rows)) {
            captured.resized.set(true);
        }
        captured.resized.get()
    });
    let live = if resized {
        panes_mode_live_frames(inner, mode.source_window)
    } else {
        BTreeMap::new()
    };
    let preview = captured
        .filter(|_| inner.modes.pane_mode_zooms.contains(&pane))
        .and_then(|captured| captured.frames.get(&pane));
    let frame = |area: &zz_mux::PanesModeAreaGeometry| {
        if !resized {
            return captured.and_then(|captured| captured.frames.get(&area.pane));
        }
        if area.pane == pane
            && let Some(preview) = preview
                .filter(|preview| area.width <= preview.columns && area.height <= preview.rows)
        {
            return Some(preview);
        }
        live.get(&area.pane)
    };
    let expand_in = |format: &str, session: SessionId, window: WindowId, shown: PaneId| {
        let mut context = engine.format_status_context_for_client(
            Some(session),
            Some(window),
            Some(shown),
            session,
        );
        context.set_format_value("config_files", &inner.config.config_files);
        let mut hooks = DaemonFormatHooks::command(facts).with_option_engine(engine);
        expand_format_values(format, &context, &mut hooks)
    };
    let expand = |format: &str, shown: PaneId| {
        expand_in(format, mode.source_session, mode.source_window, shown)
    };
    let border_style = engine
        .state
        .window_for_pane(pane)
        .and_then(|window| engine.state.windows.get(&window))
        .map_or_else(
            || options.border_style.clone(),
            |window| {
                let current = engine
                    .state
                    .sessions
                    .get(&window.session)
                    .map_or(window.id, |session| session.active_window);
                expand_in(&options.border_style, window.session, current, pane)
            },
        );
    let areas = geometry
        .areas
        .into_iter()
        .map(|area| {
            let colour = if Some(area.pane) == active {
                &options.active_colour
            } else {
                &options.colour
            };
            PanesModeArea {
                pane: area.pane,
                number: area.number,
                x: area.x,
                y: area.y,
                width: area.width,
                height: area.height,
                colour: expand(colour, area.pane),
                label: if options.format.is_empty() {
                    String::new()
                } else {
                    truncate_pane_indicator_label(expand(&options.format, area.pane))
                },
                viewport: frame(&area).map(|viewport| (**viewport).clone()),
            }
        })
        .collect();
    PaneMode::Panes {
        areas,
        borders: geometry
            .borders
            .into_iter()
            .map(|(x, y, cell)| PanesModeBorder { x, y, cell })
            .collect(),
        border_style,
        copy: geometry.copy,
        format: !options.format.is_empty(),
        clears: geometry.clears,
    }
}

fn customize_expander<'a>(
    inner: &'a ServerState,
    pane: PaneId,
    facts: &'a dyn crate::status::FormatFactSource,
) -> impl FnMut(&str, &BTreeMap<String, String>) -> String + 'a {
    let engine = &inner.engine;
    let window = engine.state.window_for_pane(pane);
    let session = window
        .and_then(|window| engine.state.windows.get(&window))
        .map(|window| window.session);
    let context = ExecutionContext::new(session, window, Some(pane));
    move |format, variables| {
        let mut hooks =
            DaemonFormatHooks::command_with_variables(facts, variables).with_option_engine(engine);
        engine.expand_pane_format(format, &context, None, FormatClient::NoClient, &mut hooks)
    }
}

fn clock_mode_time(facts: &dyn crate::status::FormatFactSource, style: u8) -> String {
    let mut hooks = DaemonFormatHooks::command(facts);
    let mut time = hooks.strftime(match style {
        0 => "%l:%M ",
        2 => "%l:%M:%S ",
        3 => "%H:%M:%S",
        _ => "%H:%M",
    });
    if style == 0 || style == 2 {
        let hour = hooks.strftime("%H").parse::<u32>().unwrap_or_default();
        time.push_str(if hour >= 12 { "PM" } else { "AM" });
    }
    time
}

fn stamp_pane_border_chrome(
    engine: &MuxEngine,
    config_files: &str,
    facts: &FormatHookFacts,
    contexts: &zz_mux::FormatContextSnapshot<'_>,
    snapshot: &mut MuxSnapshot,
) -> bool {
    let mut animated = false;
    for session in &mut snapshot.sessions {
        for window in &mut session.windows {
            window.pane_border_status = engine.displayed_pane_border_status(window.id);
            window.pane_border_lines = engine.pane_border_lines(window.id);
            window.pane_border_indicators = engine.pane_border_indicators(window.id);
            for float in &mut window.floating {
                float.border_lines = engine.pane_lines(float.pane);
                float.border_status = engine.floating_pane_border_status(float.pane);
            }
            let tiled_status = window.pane_border_status.is_on();
            let float_status: Vec<PaneId> = window
                .floating
                .iter()
                .filter(|float| float.border_status.is_on())
                .map(|float| float.pane)
                .collect();
            if !tiled_status && float_status.is_empty() {
                continue;
            }
            for (pane, pane_snapshot) in &mut window.panes {
                if !tiled_status && !float_status.contains(pane) {
                    continue;
                }
                let format = engine.pane_border_format(*pane);
                if format.is_empty() {
                    continue;
                }
                let mut context =
                    contexts.status_context(Some(session.id), Some(window.id), Some(*pane));
                context.set_format_value("config_files", config_files);
                let mut hooks = DaemonFormatHooks::command(facts)
                    .with_option_engine(engine)
                    .with_cycle();
                let (text, cycles) = crate::status::expand_cycle_frames(|| {
                    expand_status(&format, &context, &mut hooks)
                });
                pane_snapshot.border_status_text = text;
                animated |= cycles;
            }
        }
    }
    animated
}

fn stamp_pane_border_colours(
    engine: &MuxEngine,
    config_files: &str,
    facts: &FormatHookFacts,
    contexts: &zz_mux::FormatContextSnapshot<'_>,
    snapshot: &mut MuxSnapshot,
) {
    if !engine.has_pane_border_style_settings() {
        return;
    }
    let resolve = |value: Option<String>, session, window, pane| {
        let value = value?;
        let mut context = contexts.status_context(Some(session), Some(window), Some(pane));
        context.set_format_value("config_files", config_files);
        let mut hooks = DaemonFormatHooks::command(facts).with_option_engine(engine);
        let expanded = if value.contains("#{") {
            expand_format_values(&value, &context, &mut hooks)
        } else {
            value
        };
        zz_protocol::parse_style(&expanded)?.fg
    };
    for session in &mut snapshot.sessions {
        for window in &mut session.windows {
            for (pane, pane_snapshot) in &mut window.panes {
                let styles = engine.pane_border_style_values(*pane);
                pane_snapshot.border_colour = resolve(styles.border, session.id, window.id, *pane);
                pane_snapshot.active_border_colour =
                    resolve(styles.active_border, session.id, window.id, *pane);
            }
        }
    }
}

fn expand_window_status_labels(
    engine: &MuxEngine,
    config_files: &str,
    facts: &FormatHookFacts,
    contexts: &zz_mux::FormatContextSnapshot<'_>,
    snapshot: &mut MuxSnapshot,
) {
    for session in &mut snapshot.sessions {
        for window in &mut session.windows {
            let formats = engine.window_status_formats(window.id);
            let format = if window.id == session.active_window {
                &formats.current_format
            } else {
                &formats.format
            };
            let mut context = contexts.status_context(
                Some(session.id),
                Some(window.id),
                Some(window.active_pane),
            );
            context.set_format_value("config_files", config_files);
            let mut hooks = DaemonFormatHooks::command(facts).with_option_engine(engine);
            let style = expand_window_status_style(&formats, &context, &mut hooks);
            let label = expand_status(format, &context, &mut hooks);
            window.status_label =
                truncate_window_status_label(format!("#[{style}]#[push-default]{label}"));
        }
    }
}

fn expand_window_status_style(
    formats: &zz_mux::WindowStatusFormats,
    context: &zz_mux::StatusContext,
    hooks: &mut DaemonFormatHooks<'_>,
) -> String {
    let mut style = if context.variable("window_active").as_deref() == Some("1") {
        let current = expand_status(&formats.current_style, context, hooks);
        if current == "default" {
            expand_status(&formats.style, context, hooks)
        } else {
            current
        }
    } else {
        expand_status(&formats.style, context, hooks)
    };
    if context.variable("window_last_flag").as_deref() == Some("1") {
        let last = expand_status(&formats.last_style, context, hooks);
        append_window_status_style(&mut style, &last);
    }
    let mut bell_styled = false;
    if context.variable("window_bell_flag").as_deref() == Some("1") {
        let bell = expand_status(&formats.bell_style, context, hooks);
        if bell != "default" && !bell.is_empty() {
            append_window_status_style(&mut style, &bell);
            bell_styled = true;
        }
    }
    if !bell_styled
        && (context.variable("window_activity_flag").as_deref() == Some("1")
            || context.variable("window_silence_flag").as_deref() == Some("1"))
    {
        let activity = expand_status(&formats.activity_style, context, hooks);
        append_window_status_style(&mut style, &activity);
    }
    style
}

fn append_window_status_style(style: &mut String, layer: &str) {
    if layer == "default" || layer.is_empty() {
        return;
    }
    if !style.is_empty() {
        style.push(' ');
    }
    style.push_str(layer);
}

fn truncate_window_status_label(mut label: String) -> String {
    if label.len() <= MAX_WINDOW_STATUS_LABEL_BYTES {
        return label;
    }
    let boundary = (0..=MAX_WINDOW_STATUS_LABEL_BYTES)
        .rev()
        .find(|index| label.is_char_boundary(*index))
        .unwrap_or_default();
    label.truncate(boundary);
    label
}

fn truncate_pane_indicator_label(mut label: String) -> String {
    if label.len() <= MAX_PANE_INDICATOR_LABEL_BYTES {
        return label;
    }
    let boundary = (0..=MAX_PANE_INDICATOR_LABEL_BYTES)
        .rev()
        .find(|index| label.is_char_boundary(*index))
        .unwrap_or_default();
    label.truncate(boundary);
    label
}

fn prompt_key_spelling(input: &zz_terminal::KeyInput) -> String {
    if input.key == zz_terminal::KeyCode::Tab
        && input.modifiers.shift()
        && !input.modifiers.control()
        && !input.modifiers.alt()
    {
        return MENU_BACK_TAB_KEY.to_owned();
    }
    let mut name = input_key_name(input).into_string();
    if name.ends_with(' ') {
        name.pop();
        name.push_str("Space");
    }
    name
}

struct ShutdownBlocker {
    shared: Arc<Shared>,
    detached: bool,
}

impl ShutdownBlocker {
    fn acquire(shared: &Arc<Shared>, detached: bool) -> Option<Self> {
        let mut blockers = shared.shutdown_blockers.lock();
        if blockers.closed {
            return None;
        }
        blockers.active = blockers
            .active
            .checked_add(1)
            .expect("shutdown blocker count overflow");
        if detached {
            blockers.detached = blockers
                .detached
                .checked_add(1)
                .expect("detached shutdown blocker count overflow");
        }
        drop(blockers);
        Some(Self {
            shared: Arc::clone(shared),
            detached,
        })
    }

    fn fork(&self, detached: bool) -> Self {
        let mut blockers = self.shared.shutdown_blockers.lock();
        blockers.active = blockers
            .active
            .checked_add(1)
            .expect("shutdown blocker count overflow");
        if detached {
            blockers.detached = blockers
                .detached
                .checked_add(1)
                .expect("detached shutdown blocker count overflow");
        }
        drop(blockers);
        Self {
            shared: Arc::clone(&self.shared),
            detached,
        }
    }
}

impl Drop for ShutdownBlocker {
    fn drop(&mut self) {
        let resume_shutdown = {
            let mut blockers = self.shared.shutdown_blockers.lock();
            blockers.active = blockers
                .active
                .checked_sub(1)
                .expect("shutdown blocker count underflow");
            if self.detached {
                blockers.detached = blockers
                    .detached
                    .checked_sub(1)
                    .expect("detached shutdown blocker count underflow");
            }
            blockers.closed && blockers.active == 0
        };
        if resume_shutdown {
            self.shared
                .request_shutdown_with_hooks(self.shared.shutdown_forced.load(Ordering::Acquire));
        }
    }
}

#[cfg(all(test, unix))]
#[path = "daemon/shell_jobs_tests.rs"]
mod shell_jobs_tests;

#[cfg(unix)]
struct ShellJobProcess {
    pid: u32,
    cancel: Arc<AtomicBool>,
}

#[cfg(unix)]
type ShellFinish = Box<
    dyn FnOnce(
            &Arc<Shared>,
            &mut ExecutionContext,
            Option<&CommandQueueExecution>,
            Result<ShellJobResult, ()>,
        ) -> Result<Execution, DaemonError>
        + Send,
>;

#[cfg(unix)]
struct ShellWait {
    continuation: cmdq::WaitContinuation,
    result: Arc<Mutex<Option<Result<ShellJobResult, ()>>>>,
    finish: ShellFinish,
}

#[cfg(unix)]
impl ShellWait {
    fn new(
        shared: &Arc<Shared>,
        finish: impl FnOnce(
            &Arc<Shared>,
            &mut ExecutionContext,
            Option<&CommandQueueExecution>,
            Result<ShellJobResult, ()>,
        ) -> Result<Execution, DaemonError>
        + Send
        + 'static,
    ) -> (
        Self,
        impl FnOnce(Result<ShellJobResult, ()>) + Send + 'static,
    ) {
        let continuation = cmdq::WaitContinuation::new(
            shared
                .command_item
                .as_ref()
                .and_then(|item| item.lock().wait()),
            None,
        );
        let result = Arc::new(Mutex::new(None));
        let completed = continuation.clone();
        let output = Arc::clone(&result);
        let wake = shared.timer_tx.clone();
        (
            Self {
                continuation,
                result,
                finish: Box::new(finish),
            },
            move |result| {
                *output.lock() = Some(result);
                completed.complete();
                let _ = wake.send(timers::TimerInput::HookReady);
            },
        )
    }

    fn finish(
        self,
        shared: &Arc<Shared>,
        context: &mut ExecutionContext,
        queue: Option<&CommandQueueExecution>,
    ) -> Result<Execution, DaemonError> {
        let result = self.result.lock().take().expect("completed shell job");
        (self.finish)(shared, context, queue, result)
    }
}

struct ShellJobResult {
    output: Vec<u8>,
    status: ExitStatus,
}

#[cfg(all(test, unix))]
#[path = "daemon/pipe_jobs_tests.rs"]
mod pipe_jobs_tests;

#[cfg(not(unix))]
fn terminate_copy_pipe(child: &mut Child) -> (Option<String>, Option<std::io::Error>) {
    #[cfg(windows)]
    let tree_result = {
        let pid = child.id().to_string();
        std::process::Command::new("taskkill")
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status_unmasked()
            .map_err(|error| format!("could not run taskkill: {error}"))
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("taskkill exited unsuccessfully ({status})"))
                }
            })
    };
    #[cfg(not(any(unix, windows)))]
    let tree_result = Err("process-tree termination is unsupported".to_owned());
    let child_error = child.kill().err();
    let kill_error = tree_result.err().map(|tree_error| match child_error {
        Some(child_error) => {
            format!("{tree_error}; could not terminate immediate child: {child_error}")
        }
        None => tree_error,
    });
    let wait_error = child.wait().err();
    (kill_error, wait_error)
}

#[cfg(not(unix))]
fn terminate_managed_process(process: &Mutex<Option<Child>>) {
    let Some(mut child) = process.lock().take() else {
        return;
    };
    let _ = terminate_copy_pipe(&mut child);
}

#[cfg(unix)]
fn stop_pane_pipe(pipe: PanePipe) {
    if let Some(pid) = rustix::process::Pid::from_raw(pipe.pid as i32) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    pipe.cancel.store(true, Ordering::Release);
    pipe.jobs.wake.wake();
    drop(pipe);
}

#[cfg(windows)]
fn stop_pane_pipe(mut pipe: PanePipe) {
    pipe.stop.store(true, Ordering::Release);
    terminate_managed_process(&pipe.process);
    if let Some(worker) = pipe.thread.take()
        && worker.join().is_err()
    {
        log::error!("pipe-pane worker panicked for pane process {}", pipe.pid);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlOutputAgeAction {
    Output(u64),
    Pause,
    Kill,
}

fn control_output_age_action(
    pause_after_ms: Option<u64>,
    enqueued_at: Instant,
    now: Instant,
) -> ControlOutputAgeAction {
    let age = now.saturating_duration_since(enqueued_at);
    let age_ms = u64::try_from(age.as_millis()).unwrap_or(u64::MAX);
    if let Some(pause_after_ms) = pause_after_ms {
        if age_ms >= pause_after_ms {
            ControlOutputAgeAction::Pause
        } else {
            ControlOutputAgeAction::Output(age_ms)
        }
    } else if age >= CONTROL_MAXIMUM_AGE {
        ControlOutputAgeAction::Kill
    } else {
        ControlOutputAgeAction::Output(age_ms)
    }
}

fn drain_control_pane_output(
    pending: &mut VecDeque<PendingControlOutput>,
    limit: usize,
    before: u64,
    now: Instant,
    mut bytes: Vec<u8>,
) -> (u64, Vec<u8>) {
    let enqueued_at = pending
        .front()
        .expect("pending control output is present")
        .enqueued_at;
    let age_ms =
        u64::try_from(now.saturating_duration_since(enqueued_at).as_millis()).unwrap_or(u64::MAX);
    bytes.clear();
    bytes.reserve(limit);
    while bytes.len() < limit {
        let Some(chunk) = pending.front_mut().filter(|chunk| chunk.seq < before) else {
            break;
        };
        let take = (limit - bytes.len()).min(chunk.bytes.len().saturating_sub(chunk.offset));
        bytes.extend_from_slice(&chunk.bytes[chunk.offset..chunk.offset + take]);
        chunk.offset += take;
        if chunk.offset == chunk.bytes.len() {
            pending.pop_front();
        }
    }
    (age_ms, bytes)
}

#[cfg(all(test, unix))]
mod ptyleak_tests;

enum PaneSink {
    Terminal(Arc<TerminalSession>),
    Browser(PaneId),
}

enum DeferredTerminalCommand {
    SetWordSeparators {
        terminal: Arc<TerminalSession>,
        separators: WordSeparators,
    },
    SetEngineKnobs {
        terminal: Arc<TerminalSession>,
        knobs: EngineKnobs,
    },
    ArmCopySource {
        terminal: Arc<TerminalSession>,
        source: Arc<TerminalSession>,
    },
    SetAllowPassthrough {
        terminal: Arc<TerminalSession>,
        enabled: bool,
    },
    SetWrapSearch {
        terminal: Arc<TerminalSession>,
        enabled: bool,
    },
    SetAppearance {
        terminal: Arc<TerminalSession>,
        appearance: Arc<TerminalAppearance>,
    },
    AttachView {
        terminal: Arc<TerminalSession>,
        view: TerminalViewId,
    },
    Resize {
        terminal: Arc<TerminalSession>,
        geometry: TerminalGeometry,
    },
    ViewAction {
        terminal: Arc<TerminalSession>,
        view: TerminalViewId,
        action: zz_terminal::TerminalViewAction,
    },
    SendTokens {
        terminals: Vec<Arc<TerminalSession>>,
        keys: Vec<zz_protocol::KeyToken>,
        repeat: u32,
    },
    ResetScreen {
        terminals: Vec<Arc<TerminalSession>>,
    },
}

impl DeferredTerminalCommand {
    fn copy_mode_terminal(&self) -> Option<&Arc<TerminalSession>> {
        match self {
            Self::ViewAction {
                terminal, action, ..
            } if terminal_view_action_enters_copy_mode(action)
                || matches!(
                    action,
                    zz_terminal::TerminalViewAction::CopyMode(_)
                        | zz_terminal::TerminalViewAction::CopyModeCounted { .. }
                ) =>
            {
                Some(terminal)
            }
            _ => None,
        }
    }

    #[cfg(test)]
    const fn wrap_search(&self) -> Option<bool> {
        match self {
            Self::SetWrapSearch { enabled, .. } => Some(*enabled),
            _ => None,
        }
    }

    fn run(self) {
        match self {
            Self::SetWordSeparators {
                terminal,
                separators,
            } => terminal.set_word_separators(separators),
            Self::SetAllowPassthrough { terminal, enabled } => {
                terminal.set_allow_passthrough(enabled);
            }
            Self::SetWrapSearch { terminal, enabled } => terminal.set_wrap_search(enabled),
            Self::SetEngineKnobs { terminal, knobs } => terminal.set_engine_knobs(knobs),
            Self::ArmCopySource { .. } => {
                unreachable!("copy sources resume through the terminal inbox")
            }
            Self::SetAppearance {
                terminal,
                appearance,
            } => terminal.set_appearance(appearance),
            Self::AttachView { terminal, view } => terminal.attach_view(view),
            Self::ResetScreen { terminals } => {
                for terminal in terminals {
                    terminal.reset_screen();
                }
            }
            Self::Resize { terminal, geometry } => terminal.resize(
                geometry.columns,
                geometry.rows,
                geometry.cell_width_px,
                geometry.cell_height_px,
            ),
            Self::ViewAction {
                terminal,
                view,
                action,
            } => terminal.view_action(view, action),
            Self::SendTokens {
                terminals,
                keys,
                repeat,
            } => repeat_until_rejected(repeat, || send_tokens(&terminals, &keys)),
        }
    }
}

fn repeat_until_rejected(mut repeat: u32, mut send: impl FnMut() -> bool) {
    while repeat > 0 && send() {
        repeat -= 1;
    }
}

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

fn resolve_input_sinks(inner: &ServerState, source: PaneId) -> Result<Vec<PaneSink>, ServerError> {
    if inner.engine.pane_input_off(source)? {
        return Ok(Vec::new());
    }
    let targets = inner.engine.synchronized_input_targets(source)?;
    let mut sinks = Vec::with_capacity(targets.len());
    for pane in targets {
        if inner.engine.pane_input_off(pane)? {
            continue;
        }
        if matches!(
            inner.engine.state.pane(pane).map(|pane| &pane.kind),
            Some(
                zz_mux::PaneKind::Picker { .. }
                    | zz_mux::PaneKind::Editor(_)
                    | zz_mux::PaneKind::View(_)
                    | zz_mux::PaneKind::Simulator(_)
            )
        ) {
        } else if let Some(terminal) = inner.terminals.get(&pane) {
            sinks.push(PaneSink::Terminal(Arc::clone(terminal)));
        } else if matches!(
            inner.engine.state.pane(pane).map(|pane| &pane.kind),
            Some(zz_mux::PaneKind::Browser(_))
        ) {
            ensure_browser_attached(inner, pane)?;
            sinks.push(PaneSink::Browser(pane));
        } else {
            return Err(ServerError::PaneExited(pane));
        }
    }
    Ok(sinks)
}

fn host_terminal_panes(inner: &ServerState) -> BTreeSet<PaneId> {
    inner
        .engine
        .state
        .sessions
        .values()
        .flat_map(|session| session.windows.iter())
        .filter_map(|window| inner.engine.state.windows.get(window))
        .flat_map(|window| window.panes.keys())
        .filter(|pane| inner.terminals.contains_key(pane))
        .copied()
        .collect()
}

fn visible_terminal_panes(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
) -> BTreeSet<PaneId> {
    if inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Control) {
        return BTreeSet::new();
    }
    let Some(session) = inner.engine.state.sessions.get(&session) else {
        return BTreeSet::new();
    };
    let focused_window = client_focused_window(inner, client, session);
    let Some(window) = inner.engine.state.windows.get(&focused_window) else {
        return BTreeSet::new();
    };
    window
        .panes
        .iter()
        .filter(|(pane, _)| window.pane_is_visible(**pane) && inner.terminals.contains_key(pane))
        .map(|(pane, _)| *pane)
        .collect()
}

fn streamed_terminal_panes(
    inner: &ServerState,
    client: ClientId,
    visible: &BTreeSet<PaneId>,
) -> BTreeMap<PaneId, TerminalStreamKind> {
    let mut streamed = visible
        .iter()
        .copied()
        .map(|pane| (pane, TerminalStreamKind::Foreground))
        .collect::<BTreeMap<_, _>>();
    if inner.client(client).is_some_and(|c| c.terminal_preview) {
        for pane in host_terminal_panes(inner) {
            streamed.entry(pane).or_insert(TerminalStreamKind::Preview);
        }
    }
    streamed
}

#[cfg(test)]
mod test_shell;

#[cfg(test)]
mod pane_tests;

fn apply_view_streams(
    inner: &ServerState,
    view: TerminalViewId,
    previous: &BTreeMap<PaneId, TerminalStreamKind>,
    next: &BTreeMap<PaneId, TerminalStreamKind>,
) {
    let client = ClientId(view.0);
    for pane in previous.keys().filter(|pane| !next.contains_key(pane)) {
        if let Some(terminal) = inner.terminals.get(pane) {
            terminal.set_view_stream(view, ViewStream::Off);
            sync_view_sink(inner, terminal, client, *pane, None);
            if !client_is_attached_to_pane(inner, client, *pane) {
                terminal.detach_view(view);
            }
        }
    }
    for (pane, kind) in next {
        let Some(terminal) = inner.terminals.get(pane) else {
            continue;
        };
        sync_view_sink(inner, terminal, client, *pane, Some(*kind));
        if previous.contains_key(pane) {
            continue;
        }
        if !client_is_attached_to_pane(inner, client, *pane) {
            terminal.attach_view(view);
        }
        sync_view_area(inner, *pane, client);
        terminal.set_view_stream(
            view,
            match kind {
                TerminalStreamKind::Foreground => ViewStream::Foreground,
                TerminalStreamKind::Preview => ViewStream::Preview,
            },
        );
    }
}

fn sync_view_sink(
    inner: &ServerState,
    terminal: &TerminalSession,
    client: ClientId,
    pane: PaneId,
    kind: Option<TerminalStreamKind>,
) {
    let Some(sink) = shard_sink::PaneSink::of(terminal) else {
        return;
    };
    let record = inner
        .client(client)
        .filter(|_| kind == Some(TerminalStreamKind::Foreground))
        .and_then(|c| {
            let live = c
                .copy_session
                .as_ref()
                .is_none_or(|session| session.pane != pane);
            c.subscriber.clone().map(|mailbox| (mailbox, live))
        });
    sink.set_view(TerminalViewId(client.0), record);
}

fn sync_client_pane_sink(inner: &ServerState, client: ClientId, pane: PaneId) {
    let Some(terminal) = inner.terminals.get(&pane) else {
        return;
    };
    let kind = inner
        .client(client)
        .and_then(|c| c.streamed_terminals.as_ref())
        .and_then(|streamed| streamed.get(&pane))
        .copied();
    sync_view_sink(inner, terminal, client, pane, kind);
}

fn control_route_panes(inner: &ServerState, client: ClientId) -> BTreeSet<PaneId> {
    let Some(session) = client_attached_session(inner, client)
        .and_then(|session| inner.engine.state.sessions.get(&session))
    else {
        return BTreeSet::new();
    };
    session
        .windows
        .iter()
        .filter_map(|window| inner.engine.state.windows.get(window))
        .flat_map(|window| window.panes.iter())
        .filter(|(pane, _)| inner.terminals.contains_key(pane))
        .map(|(pane, _)| *pane)
        .collect()
}

fn sync_control_feed(inner: &ServerState, client: ClientId, wake: &Arc<shard_sink::ControlWake>) {
    let Some(state) = inner
        .client(client)
        .filter(|c| c.kind == Some(ClientKind::Control))
    else {
        return;
    };
    let Some(mailbox) = state.subscriber.as_ref() else {
        return;
    };
    let output = state.control_output.as_ref();
    mailbox.ensure_control_feed(wake).configure(
        output.is_some_and(|output| output.no_output),
        output.and_then(|output| output.pause_after_ms),
        output
            .map(|output| {
                output
                    .panes
                    .iter()
                    .filter(|(_, pane)| pane.mode != ControlPaneOutputMode::On)
                    .map(|(pane, _)| *pane)
                    .collect()
            })
            .unwrap_or_default(),
    );
}

fn sync_control_routes(inner: &mut ServerState, wake: &Arc<shard_sink::ControlWake>) {
    let desired = inner
        .clients
        .iter()
        .filter(|(_, state)| state.kind == Some(ClientKind::Control))
        .filter_map(|(client, state)| {
            let mailbox = Arc::clone(state.subscriber.as_ref()?);
            Some((*client, (mailbox, control_route_panes(inner, *client))))
        })
        .collect::<BTreeMap<_, _>>();
    if desired.is_empty() && inner.attachment.control_routes.is_empty() {
        return;
    }
    let previous = std::mem::take(&mut inner.attachment.control_routes);
    let sink = |pane: &PaneId| {
        inner
            .terminals
            .get(pane)
            .and_then(|terminal| shard_sink::PaneSink::of(terminal))
    };
    for (client, route) in &previous {
        let kept = desired
            .get(client)
            .filter(|(mailbox, _)| Arc::ptr_eq(mailbox, &route.mailbox));
        for pane in &route.panes {
            if kept.is_some_and(|(_, panes)| panes.contains(pane)) {
                continue;
            }
            if let Some(sink) = sink(pane) {
                sink.set_control(&route.mailbox, false);
            }
        }
    }
    for (client, (mailbox, panes)) in &desired {
        sync_control_feed(inner, *client, wake);
        let known = previous
            .get(client)
            .filter(|route| Arc::ptr_eq(&route.mailbox, mailbox));
        for pane in panes {
            if known.is_some_and(|route| route.panes.contains(pane)) {
                continue;
            }
            if let Some(sink) = sink(pane) {
                sink.set_control(mailbox, true);
            }
        }
    }
    inner.attachment.control_routes = desired
        .into_iter()
        .map(|(client, (mailbox, panes))| (client, ControlRoute { mailbox, panes }))
        .collect();
}

#[cfg(test)]
fn output_routed(inner: &ServerState, pane: PaneId) -> bool {
    inner
        .terminals
        .get(&pane)
        .and_then(|terminal| shard_sink::PaneSink::of(terminal))
        .is_some_and(zz_terminal::TerminalFrameSink::takes_output)
}

fn retire_terminal(terminal: &TerminalSession) {
    terminal.retire();
    if let Some(sink) = shard_sink::PaneSink::of(terminal) {
        sink.clear();
    }
}

fn sync_terminal_freeze(inner: &ServerState, client: ClientId) {
    if let Some(subscriber) = inner.client(client).and_then(|c| c.subscriber.as_ref()) {
        subscriber.terminals_frozen.store(
            client_terminal_publication_frozen(inner, client),
            Ordering::Release,
        );
    }
}

fn chooser_preview_panes(inner: &ServerState) -> BTreeSet<PaneId> {
    let state = &inner.engine.state;
    let mut panes = BTreeSet::new();
    for chooser in inner
        .clients
        .values()
        .filter_map(|c| c.choose_tree.as_ref())
        .filter(|chooser| chooser.preview_size != ChooserPreviewSize::Off)
    {
        let Some(item) = usize::try_from(chooser.rendered.selected)
            .ok()
            .and_then(|selected| chooser.rendered.items.get(selected))
        else {
            continue;
        };
        match item.target {
            ChooseTreeTarget::Session(session) => panes.extend(
                state
                    .sessions
                    .get(&session)
                    .into_iter()
                    .flat_map(|session| session.windows.iter())
                    .filter_map(|window| state.windows.get(window))
                    .map(|window| window.active_pane),
            ),
            ChooseTreeTarget::Window(window) => panes.extend(
                state
                    .windows
                    .get(&window)
                    .into_iter()
                    .flat_map(|window| window.panes.keys().copied()),
            ),
            ChooseTreeTarget::Pane(pane) => {
                panes.insert(pane);
            }
            ChooseTreeTarget::Client(client) if !chooser.info_preview => panes.extend(
                chooser
                    .clients
                    .iter()
                    .filter(|row| row.client == client)
                    .filter_map(|row| row.pane)
                    .filter(|pane| !(chooser.hide_source && *pane == chooser.source_pane)),
            ),
            ChooseTreeTarget::Client(_) => {}
        }
    }
    panes.retain(|pane| inner.terminals.contains_key(pane));
    panes
}

fn refresh_preview_watches(inner: &mut ServerState) {
    let next = chooser_preview_panes(inner);
    if next == inner.panes.preview_watched {
        return;
    }
    for pane in inner.panes.preview_watched.symmetric_difference(&next) {
        if let Some(terminal) = inner.terminals.get(pane) {
            terminal.set_preview_watch(next.contains(pane));
        }
    }
    inner.panes.preview_watched = next;
}

#[cfg(feature = "agent")]
fn visible_agent_panes(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
) -> BTreeSet<PaneId> {
    let Some(session) = inner.engine.state.sessions.get(&session) else {
        return BTreeSet::new();
    };
    let focused_window = client_focused_window(inner, client, session);
    let Some(window) = inner.engine.state.windows.get(&focused_window) else {
        return BTreeSet::new();
    };
    window
        .panes
        .keys()
        .filter(|pane| {
            window.zoomed_pane.is_none_or(|zoomed| **pane == zoomed)
                && inner.agents.agent_panes.contains(*pane)
        })
        .copied()
        .collect()
}

fn attached_clients_for_pane(inner: &ServerState, pane: PaneId) -> Option<&BTreeSet<ClientId>> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let session = inner.engine.state.windows.get(&window)?.session;
    inner.attachment.attached.get(&session)
}

fn client_is_attached_to_pane(inner: &ServerState, client: ClientId, pane: PaneId) -> bool {
    attached_clients_for_pane(inner, pane).is_some_and(|clients| clients.contains(&client))
}

fn sync_view_area(inner: &ServerState, pane: PaneId, client: ClientId) {
    if let Some(terminal) = inner.terminals.get(&pane) {
        terminal.set_view_area(
            TerminalViewId(client.0),
            client_display_area(inner, client, pane),
        );
    }
}

fn sync_client_view_areas(inner: &ServerState, client: ClientId) {
    for pane in inner
        .client(client)
        .and_then(|c| c.streamed_terminals.as_ref())
        .into_iter()
        .flat_map(BTreeMap::keys)
    {
        sync_view_area(inner, *pane, client);
    }
}

fn client_display_area(inner: &ServerState, client: ClientId, pane: PaneId) -> Option<(u16, u16)> {
    if let Some(size) = inner.client(client)?.size {
        return Some(size);
    }
    let window = inner.engine.state.window_for_pane(pane)?;
    inner
        .panes
        .terminal_geometries
        .iter()
        .filter(|(reported, _)| inner.engine.state.window_for_pane(**reported) == Some(window))
        .filter_map(|(reported, geometries)| {
            let geometry = geometries.get(&client)?;
            inner
                .engine
                .window_extent_for_pane_geometry(*reported, geometry.columns, geometry.rows)
        })
        .reduce(|(columns, rows), (other_columns, other_rows)| {
            (columns.max(other_columns), rows.max(other_rows))
        })
}

fn terminal_geometry_owner(inner: &ServerState, pane: PaneId) -> Option<ClientId> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let session = inner.engine.state.windows.get(&window)?.session;
    inner
        .attachment
        .attached
        .get(&session)?
        .iter()
        .filter(|client| {
            inner.client(**client).and_then(|c| c.kind) == Some(ClientKind::Control)
                || inner
                    .client(**client)
                    .and_then(|c| c.visible_terminals.as_ref())
                    .is_some_and(|visible| visible.contains(&pane))
        })
        .filter(|client| client_terminal_geometry(inner, **client, pane).is_some())
        .min_by_key(|client| {
            (
                Reverse(
                    inner
                        .client(**client)
                        .and_then(|c| c.terminal_input_sequence.as_ref())
                        .copied()
                        .unwrap_or_default(),
                ),
                client.0,
            )
        })
        .copied()
}

fn window_geometry_owner(
    inner: &ServerState,
    window: &zz_mux::Window,
) -> Option<(PaneId, ClientId)> {
    window
        .panes
        .keys()
        .find_map(|pane| Some((*pane, terminal_geometry_owner(inner, *pane)?)))
}

fn client_terminal_geometry(
    inner: &ServerState,
    client: ClientId,
    pane: PaneId,
) -> Option<TerminalGeometry> {
    if inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Control) {
        return inner
            .panes
            .terminal_geometries
            .get(&pane)?
            .get(&client)
            .copied();
    }
    let window = inner.engine.state.window_for_pane(pane)?;
    let session = inner.engine.state.windows.get(&window)?.session;
    if client_attached_session(inner, client) != Some(session) {
        return None;
    }
    let geometry = control_client_geometry(inner, client, window)?;
    let (columns, rows) =
        inner
            .engine
            .pane_geometry_at_window_extent(pane, geometry.columns, geometry.rows)?;
    Some(TerminalGeometry {
        columns,
        rows,
        ..geometry
    })
}

#[cfg(test)]
fn aggressive_terminal_geometry(inner: &ServerState, pane: PaneId) -> Option<TerminalGeometry> {
    terminal_geometry_for_mode(inner, pane, true, WindowSize::Smallest)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GeometrySource {
    ClientReport,
    LaidOut,
}

#[cfg(test)]
fn terminal_geometry_for_mode(
    inner: &ServerState,
    pane: PaneId,
    aggressive: bool,
    mode: WindowSize,
) -> Option<TerminalGeometry> {
    terminal_geometry_for_mode_from(inner, pane, aggressive, mode, GeometrySource::ClientReport)
}

fn terminal_geometry_for_mode_from(
    inner: &ServerState,
    pane: PaneId,
    aggressive: bool,
    mode: WindowSize,
    source: GeometrySource,
) -> Option<TerminalGeometry> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let window_state = inner.engine.state.windows.get(&window)?;
    let floating = window_state.is_floating(pane);
    if floating && (source == GeometrySource::LaidOut || window_state.zoomed_pane != Some(pane)) {
        return floating_terminal_geometry(inner, window_state, pane);
    }
    if window_state
        .zoomed_pane
        .is_some_and(|zoomed| zoomed != pane)
    {
        return None;
    }
    let session = window_state.session;
    let suppress_ignored = unignored_attached_sizing_client_exists(inner);
    let place = (source == GeometrySource::ClientReport)
        .then(|| pane_place(inner, pane))
        .flatten();
    let drawn = !inner.terminals.contains_key(&pane);
    let candidates = inner
        .attachment
        .attached
        .get(&session)?
        .iter()
        .filter(|client| client_is_sizing_candidate(inner, **client, suppress_ignored))
        .filter(|client| {
            if aggressive || drawn {
                client_focused_window_for_attachment(inner, **client) == Some(window)
            } else {
                inner.client(**client).and_then(|c| c.kind) == Some(ClientKind::Control)
                    || inner
                        .client(**client)
                        .and_then(|c| c.visible_terminals.as_ref())
                        .is_some_and(|visible| visible.contains(&pane))
            }
        })
        .filter_map(|client| {
            let geometry = client_terminal_geometry(inner, *client, pane)?;
            let reported = inner
                .panes
                .reported_pane_places
                .get(&pane)
                .and_then(|places| places.get(client));
            if floating && reported.map(|reported| &reported.place) != place.as_ref() {
                return None;
            }
            let Some((columns, rows)) = place.as_ref().and_then(|place| {
                let reported = reported?;
                (reported.place != *place).then_some(reported.extent)
            }) else {
                return Some((*client, geometry));
            };
            let (columns, rows) = inner
                .engine
                .pane_geometry_at_window_extent(pane, columns, rows)?;
            Some((
                *client,
                TerminalGeometry {
                    columns,
                    rows,
                    ..geometry
                },
            ))
        })
        .collect::<Vec<_>>();
    let owner = candidates.iter().min_by_key(|(client, _)| {
        (
            Reverse(
                inner
                    .client(*client)
                    .and_then(|c| c.terminal_input_sequence.as_ref())
                    .copied()
                    .unwrap_or_default(),
            ),
            client.0,
        )
    })?;
    let mut geometry = if mode == WindowSize::Manual || source == GeometrySource::LaidOut {
        let (columns, rows) = inner.engine.pane_geometry(pane)?;
        TerminalGeometry {
            columns,
            rows,
            ..owner.1
        }
    } else if mode == WindowSize::Latest {
        owner.1
    } else {
        let first_geometry = candidates.first()?.1;
        let mut columns = first_geometry.columns;
        let mut rows = first_geometry.rows;
        for (_, geometry) in &candidates[1..] {
            if mode == WindowSize::Largest {
                columns = columns.max(geometry.columns);
                rows = rows.max(geometry.rows);
            } else {
                columns = columns.min(geometry.columns);
                rows = rows.min(geometry.rows);
            }
        }
        TerminalGeometry {
            columns,
            rows,
            ..owner.1
        }
    };
    if mode != WindowSize::Manual {
        for (client, _) in &candidates {
            let Some(ceiling) = inner
                .client(*client)
                .and_then(|c| c.control_output.as_ref())
                .and_then(|output| output.window_geometries.get(&window))
            else {
                continue;
            };
            let (columns, rows) =
                inner
                    .engine
                    .pane_geometry_at_window_extent(pane, ceiling.columns, ceiling.rows)?;
            geometry.columns = geometry.columns.min(columns);
            geometry.rows = geometry.rows.min(rows);
        }
    }
    Some(geometry)
}

fn floating_terminal_geometry(
    inner: &ServerState,
    window: &zz_mux::Window,
    pane: PaneId,
) -> Option<TerminalGeometry> {
    if !window.pane_is_visible(pane) {
        return None;
    }
    let (columns, rows) = inner.engine.pane_geometry(pane)?;
    let (cell_width_px, cell_height_px) = window_geometry_owner(inner, window)
        .and_then(|(reported, owner)| client_terminal_geometry(inner, owner, reported))
        .map(|geometry| (geometry.cell_width_px, geometry.cell_height_px))
        .or_else(|| {
            inner
                .attachment
                .attached
                .get(&window.session)
                .into_iter()
                .flatten()
                .find_map(|client| inner.client(*client).and_then(|c| c.cell_pixels))
        })
        .unwrap_or_default();
    Some(TerminalGeometry {
        columns,
        rows,
        cell_width_px,
        cell_height_px,
    })
}

fn terminal_resize_for_pane(
    inner: &ServerState,
    pane: PaneId,
) -> Option<(Arc<TerminalSession>, TerminalGeometry)> {
    let geometry = pane_geometry_from(inner, pane, GeometrySource::LaidOut)?;
    let terminal = inner.terminals.get(&pane).cloned()?;
    Some((terminal, geometry))
}

fn pane_geometry_from(
    inner: &ServerState,
    pane: PaneId,
    source: GeometrySource,
) -> Option<TerminalGeometry> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let aggressive = inner.engine.state.window_aggressive_resize(window).ok()?;
    let mode = inner.engine.window_size(window);
    terminal_geometry_for_mode_from(inner, pane, aggressive, mode, source)
}

#[derive(Clone, Debug, PartialEq)]
struct PanePlace {
    path: Vec<(SplitId, bool)>,
    zoomed: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct ReportedPlace {
    place: PanePlace,
    extent: (u16, u16),
}

fn record_report_place(
    inner: &mut ServerState,
    client: ClientId,
    pane: PaneId,
    extent: Option<(u16, u16)>,
) {
    let reported = pane_place(inner, pane)
        .zip(extent)
        .map(|(place, extent)| ReportedPlace { place, extent });
    let places = inner.panes.reported_pane_places.entry(pane).or_default();
    match reported {
        Some(reported) => {
            places.insert(client, reported);
        }
        None => {
            places.remove(&client);
        }
    }
}

fn pane_place(inner: &ServerState, pane: PaneId) -> Option<PanePlace> {
    let window = inner.engine.state.window_for_pane(pane)?;
    let window = inner.engine.state.windows.get(&window)?;
    if window.is_floating(pane) {
        return Some(PanePlace {
            path: Vec::new(),
            zoomed: window.zoomed_pane == Some(pane),
        });
    }
    let mut node = window.layout.project();
    let mut path = Vec::new();
    loop {
        match node {
            zz_protocol::LayoutNode::Pane(found) => {
                return (found == pane).then(|| PanePlace {
                    path,
                    zoomed: window.zoomed_pane == Some(pane),
                });
            }
            zz_protocol::LayoutNode::Split {
                id, first, second, ..
            } => {
                let in_first = first.contains(pane);
                path.push((id, in_first));
                node = if in_first { *first } else { *second };
            }
            zz_protocol::LayoutNode::Empty => return None,
        }
    }
}

fn write_back_terminal_geometries(inner: &mut ServerState, panes: &BTreeSet<PaneId>) -> bool {
    let measurements = panes
        .iter()
        .filter_map(|pane| {
            if !inner.terminals.contains_key(pane) {
                return None;
            }
            pane_geometry_from(inner, *pane, GeometrySource::ClientReport)
                .map(|geometry| (*pane, geometry.columns, geometry.rows))
        })
        .collect::<Vec<_>>();
    measurements
        .into_iter()
        .fold(false, |changed, (pane, columns, rows)| {
            inner.engine.set_pane_geometry(pane, columns, rows) || changed
        })
}

fn mark_client_terminal_latest(inner: &mut ServerState, client: ClientId) {
    let sequence = inner
        .panes
        .terminal_input_sequence
        .checked_add(1)
        .expect("terminal input sequence exhausted");
    inner.panes.terminal_input_sequence = sequence;
    inner
        .client_entry(client)
        .terminal_input_sequence
        .replace(sequence);
}

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

fn mouse_pane_cell(
    inner: &ServerState,
    pane: PaneId,
    mouse: &MouseEventTarget,
) -> Option<(usize, usize)> {
    let geometry = inner.engine.pane_format_geometry(None, None, Some(pane));
    let (left, top) = (geometry.pane_left?, geometry.pane_top?);
    let (x, y) = (mouse.column.checked_sub(left)?, mouse.row.checked_sub(top)?);
    if geometry.pane_right.is_none_or(|right| mouse.column > right)
        || geometry.pane_bottom.is_none_or(|bottom| mouse.row > bottom)
    {
        return None;
    }
    Some((usize::from(x), usize::from(y)))
}

#[cfg(all(test, unix))]
fn wait_for_terminal_identity(terminal: &TerminalSession) {
    terminal.wait_for_identity(Duration::from_secs(2));
}

fn terminal_resizes_for_panes(
    inner: &ServerState,
    panes: &BTreeSet<PaneId>,
) -> Vec<(Arc<TerminalSession>, TerminalGeometry)> {
    panes
        .iter()
        .filter_map(|pane| terminal_resize_for_pane(inner, *pane))
        .collect()
}

fn apply_terminal_resizes(resizes: Vec<(Arc<TerminalSession>, TerminalGeometry)>) {
    for (terminal, geometry) in resizes {
        terminal.resize(
            geometry.columns,
            geometry.rows,
            geometry.cell_width_px,
            geometry.cell_height_px,
        );
    }
}

fn chooser_row_for_key<T>(
    items: &[T],
    input: &zz_terminal::KeyInput,
    searching: bool,
    key: impl Fn(&T) -> &str,
) -> Option<u32> {
    if searching {
        return None;
    }
    let pressed = input_key_name(input);
    let pressed = pressed.as_str();
    if pressed.is_empty() {
        return None;
    }
    let row = items
        .iter()
        .position(|item| !key(item).is_empty() && key(item) == pressed)?;
    u32::try_from(row).ok()
}

fn default_chooser_row_key(line: usize) -> String {
    match line {
        0..=9 => line.to_string(),
        10..=35 => {
            let offset = u8::try_from(line - 10).expect("a row below 36 fits a byte");
            format!("M-{}", char::from(b'a' + offset))
        }
        _ => String::new(),
    }
}

fn parsed_chooser_row_key(expanded: &str) -> String {
    let canonical = canonical_key(expanded);
    if canonical.len() > MAX_CHOOSE_ITEM_KEY_BYTES || !is_key_name(&canonical) {
        return String::new();
    }
    canonical
}

const CHOOSER_ROW_CONTEXT_FORMATS: [&str; 1] = ["line"];

fn chooser_row_variables(line: usize) -> BTreeMap<String, String> {
    let [line_name] = CHOOSER_ROW_CONTEXT_FORMATS;
    BTreeMap::from([(line_name.to_owned(), line.to_string())])
}

struct PanesModeFrames {
    size: Option<(u16, u16)>,
    resized: std::cell::Cell<bool>,
    frames: BTreeMap<PaneId, Arc<zz_terminal::TerminalViewport>>,
}

fn panes_mode_live_frames(
    inner: &ServerState,
    window: WindowId,
) -> BTreeMap<PaneId, Arc<zz_terminal::TerminalViewport>> {
    inner
        .engine
        .state
        .windows
        .get(&window)
        .map(|state| {
            state
                .pane_order()
                .iter()
                .filter_map(|shown| {
                    inner
                        .terminals
                        .get(shown)
                        .map(|terminal| (*shown, terminal.latest_viewport()))
                })
                .collect()
        })
        .unwrap_or_default()
}

struct PaneModeTransition {
    pane: PaneId,
    entered: bool,
    previous: Option<&'static str>,
    current: Option<&'static str>,
}

fn pane_mode_name(mode: &PaneModeRequest) -> &'static str {
    match mode {
        PaneModeRequest::Clock => "clock-mode",
        PaneModeRequest::Customize(_) => "options-mode",
        PaneModeRequest::Switch(_) => "switch-mode",
        PaneModeRequest::Panes(_) => "panes-mode",
    }
}

fn top_pane_mode_name(inner: &ServerState, pane: PaneId) -> Option<&'static str> {
    inner
        .modes
        .pane_modes
        .get(&pane)
        .and_then(|modes| modes.last())
        .map(pane_mode_name)
}

fn take_pane_mode_hook_events(inner: &mut ServerState) -> Vec<PendingHookEvent> {
    let transitions = std::mem::take(&mut inner.modes.pane_mode_transitions);
    hook_events::server_mode_hook_events(&inner.engine, transitions)
}

fn pane_mode_lifetime(mode: &PaneModeRequest) -> (bool, bool) {
    match mode {
        PaneModeRequest::Clock => (false, false),
        PaneModeRequest::Customize(mode) => (mode.kill_source, mode.zoom),
        PaneModeRequest::Switch(mode) => (mode.kill_source, mode.zoom),
        PaneModeRequest::Panes(mode) => (mode.kill_source, mode.zoom),
    }
}

fn pop_pane_mode(inner: &mut ServerState, pane: PaneId) -> bool {
    let Some(modes) = inner.modes.pane_modes.get_mut(&pane) else {
        return false;
    };
    let mode = modes.pop();
    if modes.is_empty() {
        inner.modes.pane_modes.remove(&pane);
    }
    if matches!(mode, Some(PaneModeRequest::Panes(_))) {
        inner.modes.panes_mode_frames.remove(&pane);
        inner.engine.set_pane_status_hidden(pane, false);
    }
    if let Some(popped) = mode.as_ref() {
        let current = top_pane_mode_name(inner, pane);
        inner.modes.pane_mode_transitions.push(PaneModeTransition {
            pane,
            entered: false,
            previous: Some(pane_mode_name(popped)),
            current,
        });
    }
    if let Some((kill_source, _)) = mode
        .as_ref()
        .filter(|mode| !matches!(mode, PaneModeRequest::Clock))
        .map(pane_mode_lifetime)
    {
        let zoom_held = inner
            .modes
            .pane_modes
            .get(&pane)
            .is_some_and(|modes| modes.iter().any(|mode| pane_mode_lifetime(mode).1));
        if !zoom_held
            && inner.modes.pane_mode_zooms.remove(&pane)
            && let Some(window) = inner.engine.state.window_for_pane(pane)
            && let Some(zoomed) = inner.engine.state.windows[&window].zoomed_pane
        {
            let _ = inner.engine.state.toggle_zoom(zoomed);
        }
        if kill_source {
            inner.modes.pane_mode_kill_panes.push(pane);
        }
    }
    mode.is_some()
}

fn consume_pane_mode_mouse(
    inner: &mut ServerState,
    pane: PaneId,
    action: Option<&zz_terminal::TerminalViewAction>,
) -> bool {
    let Some(modes) = inner.modes.pane_modes.get_mut(&pane) else {
        return false;
    };
    if matches!(action, Some(zz_terminal::TerminalViewAction::Mouse(input))
        if input.phase() == zz_terminal::TerminalMousePhase::Motion && input.button().is_none())
    {
        return true;
    }
    if matches!(modes.last(), Some(PaneModeRequest::Clock)) {
        modes.pop();
        if modes.is_empty() {
            inner.modes.pane_modes.remove(&pane);
        }
        let current = top_pane_mode_name(inner, pane);
        inner.modes.pane_mode_transitions.push(PaneModeTransition {
            pane,
            entered: false,
            previous: Some("clock-mode"),
            current,
        });
    }
    true
}

fn clock_mode_timer_needed(inner: &ServerState) -> bool {
    inner.modes.pane_modes.iter().any(|(pane, modes)| {
        matches!(modes.last(), Some(PaneModeRequest::Clock))
            && inner
                .engine
                .state
                .window_for_pane(*pane)
                .and_then(|window| inner.engine.state.windows.get(&window))
                .is_some_and(|window| {
                    inner.clients.iter().any(|(id, client)| {
                        client.subscriber.is_some()
                            && client_attached_session(inner, *id) == Some(window.session)
                            && client
                                .ctrl_subscriptions
                                .as_ref()
                                .is_none_or(|subscriptions| {
                                    subscriptions.tree != zz_protocol::TreeSubscription::None
                                })
                    })
                })
    })
}

fn duration_to_next_second() -> Duration {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Duration::from_nanos(u64::from(1_000_000_000 - now.subsec_nanos()))
}

fn pane_mode_format_facts(inner: &ServerState) -> BTreeMap<PaneId, (usize, &'static str)> {
    inner
        .modes
        .pane_modes
        .iter()
        .filter_map(|(pane, modes)| {
            Some((
                *pane,
                (
                    modes.len(),
                    match modes.last()? {
                        PaneModeRequest::Clock => "clock-mode",
                        PaneModeRequest::Customize(_) => "options-mode",
                        PaneModeRequest::Switch(_) => "switch-mode",
                        PaneModeRequest::Panes(_) => "panes-mode",
                    },
                ),
            ))
        })
        .collect()
}

#[cfg(test)]
mod format_tree_tests;

#[derive(Clone, Copy)]
struct ClientFormatFields<'a> {
    clients: &'a BTreeMap<ClientId, Box<Client>>,
    attached: &'a BTreeMap<SessionId, BTreeSet<ClientId>>,
    client_flags: &'a ClientFlags,
    terminal_geometries: &'a BTreeMap<PaneId, BTreeMap<ClientId, TerminalGeometry>>,
    terminals: &'a Arc<BTreeMap<PaneId, Arc<TerminalSession>>>,
}

impl<'a> ClientFormatFields<'a> {
    fn from_inner(inner: &'a ServerState) -> Self {
        Self {
            clients: &inner.clients,
            attached: &inner.attachment.attached,
            client_flags: &inner.attachment.client_flags,
            terminal_geometries: &inner.panes.terminal_geometries,
            terminals: &inner.terminals,
        }
    }
}

struct ClientFormatSource<'a> {
    engine: &'a MuxEngine,
    fields: ClientFormatFields<'a>,
}

impl<'a> ClientFormatSource<'a> {
    fn from_inner(inner: &'a ServerState) -> Self {
        Self {
            engine: &inner.engine,
            fields: ClientFormatFields::from_inner(inner),
        }
    }
}

impl<'a> std::ops::Deref for ClientFormatSource<'a> {
    type Target = ClientFormatFields<'a>;
    fn deref(&self) -> &Self::Target {
        &self.fields
    }
}

#[derive(Default)]
struct CommandFormatSeed {
    client: Option<ClientFormatFacts>,
    invoking: Option<ClientId>,
}

#[derive(Default)]
struct LazyDerivedFormatFacts {
    clients: std::cell::OnceCell<Vec<zz_mux::FormatClientRow>>,
    buffer: std::cell::OnceCell<Option<BufferFormatFacts>>,
    pane_pipes: std::cell::OnceCell<BTreeMap<PaneId, u32>>,
    session_attachments: std::cell::OnceCell<BTreeMap<SessionId, (usize, String)>>,
    unseen_changes: std::cell::OnceCell<BTreeSet<PaneId>>,
    window_clients: std::cell::OnceCell<BTreeMap<WindowId, Vec<String>>>,
    copy_modes:
        std::cell::OnceCell<BTreeMap<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>>>,
    pane_modes: std::cell::OnceCell<BTreeMap<PaneId, (usize, &'static str)>>,
}

struct BorrowedFormatHookFacts<'a> {
    client_fields: ClientFormatFields<'a>,
    paste_buffers: &'a [PasteBuffer],
    terminals: &'a BTreeMap<PaneId, Arc<TerminalSession>>,
    pane_pipes: &'a BTreeMap<PaneId, PanePipe>,
    attached: &'a BTreeMap<SessionId, BTreeSet<ClientId>>,
    clients: &'a BTreeMap<ClientId, Box<Client>>,
    session_last_attached: &'a BTreeMap<SessionId, u64>,
    pane_modes: &'a BTreeMap<PaneId, Vec<PaneModeRequest>>,
    seed: CommandFormatSeed,
    derived: LazyDerivedFormatFacts,
}

impl BorrowedFormatHookFacts<'_> {
    fn client_name(&self, client: ClientId) -> String {
        self.clients
            .get(&client)
            .and_then(|c| c.tty.as_ref())
            .filter(|tty| !tty.is_empty())
            .cloned()
            .or_else(|| {
                self.clients
                    .get(&client)
                    .and_then(|c| c.name.as_ref())
                    .filter(|name| !name.is_empty())
                    .cloned()
            })
            .unwrap_or_else(|| {
                let pid = self
                    .clients
                    .get(&client)
                    .and_then(|c| c.pid)
                    .unwrap_or_default();
                if pid == 0 {
                    format!("device-{}", client.0)
                } else {
                    format!("client-{pid}")
                }
            })
    }
}

fn pane_in_mode_count_from_parts(
    pane: PaneId,
    pane_modes: &BTreeMap<PaneId, Vec<PaneModeRequest>>,
    clients: &BTreeMap<ClientId, Box<Client>>,
    terminals: &BTreeMap<PaneId, Arc<TerminalSession>>,
) -> usize {
    pane_modes.get(&pane).map_or(0, Vec::len)
        + usize::from(
            clients
                .iter()
                .filter_map(|(id, client)| {
                    client.copy_session.as_ref().map(|session| (id, session))
                })
                .any(|(client, session)| {
                    session.pane == pane
                        && !session.exiting
                        && terminals.get(&pane).is_some_and(|terminal| {
                            terminal.copy_mode_facts(TerminalViewId(client.0)).is_some()
                        })
                }),
        )
}

impl crate::status::FormatFactSource for BorrowedFormatHookFacts<'_> {
    fn terminals(&self) -> &BTreeMap<PaneId, Arc<TerminalSession>> {
        self.terminals
    }
    fn session_last_attached(&self) -> &BTreeMap<SessionId, u64> {
        self.session_last_attached
    }
    fn buffer(&self) -> Option<&BufferFormatFacts> {
        self.derived
            .buffer
            .get_or_init(|| {
                self.paste_buffers
                    .iter()
                    .find(|buffer| buffer.automatic)
                    .map(buffer_format_facts)
            })
            .as_ref()
    }
    fn client(&self) -> Option<&ClientFormatFacts> {
        self.seed.client.as_ref()
    }
    fn clients(&self, context: &zz_mux::StatusContext) -> &[zz_mux::FormatClientRow] {
        self.derived.clients.get_or_init(|| {
            let Some(engine) = context.engine() else {
                return Vec::new();
            };
            let source = ClientFormatSource {
                engine,
                fields: self.client_fields,
            };
            loop_clients_in_connect_order(self.attached, engine)
                .into_iter()
                .map(|(client, session)| {
                    client_format_facts_from_source(&source, client, session).loop_row(
                        self.client_fields
                            .clients
                            .get(&client)
                            .and_then(|c| c.activity)
                            .unwrap_or_default(),
                    )
                })
                .collect()
        })
    }
    fn client_environment(&self) -> Option<&Arc<ClientEnvironmentBlob>> {
        self.seed.invoking.and_then(|client| {
            self.client_fields
                .clients
                .get(&client)
                .and_then(|c| c.environment.as_ref())
        })
    }
    fn message(&self) -> Option<&crate::status::MessageFormatFacts> {
        None
    }
    fn mux(&self) -> &zz_mux::FormatFacts {
        &hook_events::unread_format_facts().mux
    }

    fn pane_pipes(&self) -> &BTreeMap<PaneId, u32> {
        self.derived.pane_pipes.get_or_init(|| {
            self.pane_pipes
                .iter()
                .map(|(pane, pipe)| (*pane, pipe.pid))
                .collect()
        })
    }

    fn session_attachments(&self) -> &BTreeMap<SessionId, (usize, String)> {
        self.derived.session_attachments.get_or_init(|| {
            self.attached
                .iter()
                .map(|(session, clients)| {
                    let clients = clients
                        .iter()
                        .filter(|client| !self.clients.get(*client).is_some_and(|c| c.suspended))
                        .copied()
                        .collect::<Vec<_>>();
                    let names = clients
                        .iter()
                        .map(|client| self.client_name(*client))
                        .collect::<Vec<_>>()
                        .join(",");
                    (*session, (clients.len(), names))
                })
                .collect()
        })
    }

    fn unseen_changes(&self) -> &BTreeSet<PaneId> {
        self.derived.unseen_changes.get_or_init(|| {
            self.clients
                .values()
                .filter_map(|c| c.copy_session.as_ref())
                .filter(|session| !session.exiting && session.unseen)
                .map(|session| session.pane)
                .collect()
        })
    }

    fn window_clients(&self, context: &zz_mux::StatusContext) -> &BTreeMap<WindowId, Vec<String>> {
        self.derived.window_clients.get_or_init(|| {
            let Some(engine) = context.engine() else {
                return BTreeMap::new();
            };
            let source = ClientFormatSource {
                engine,
                fields: self.client_fields,
            };
            self.attached.values().flatten().fold(
                BTreeMap::<WindowId, Vec<String>>::new(),
                |mut windows, client| {
                    if let Some(session) = client_attached_session_from_source(&source, *client)
                        .and_then(|session| engine.state.sessions.get(&session))
                    {
                        let window = client_focused_window_from_source(&source, *client, session);
                        windows
                            .entry(window)
                            .or_default()
                            .push(self.client_name(*client));
                    }
                    windows
                },
            )
        })
    }

    fn copy_modes(&self) -> &BTreeMap<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>> {
        self.derived.copy_modes.get_or_init(|| {
            let mut panes =
                BTreeMap::<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>>::new();
            for (client, session) in self.clients.iter().filter_map(|(id, client)| {
                client.copy_session.as_ref().map(|session| (id, session))
            }) {
                if session.exiting {
                    continue;
                }
                if let Some(facts) = self
                    .terminals
                    .get(&session.pane)
                    .and_then(|terminal| terminal.copy_mode_facts(TerminalViewId(client.0)))
                {
                    let name = self
                        .clients
                        .get(client)
                        .and_then(|c| c.name.as_ref())
                        .cloned()
                        .unwrap_or_default();
                    panes.entry(session.pane).or_default().push((name, facts));
                }
            }
            panes
        })
    }

    fn pane_modes(&self) -> &BTreeMap<PaneId, (usize, &'static str)> {
        self.derived.pane_modes.get_or_init(|| {
            self.pane_modes
                .iter()
                .filter_map(|(pane, modes)| {
                    Some((
                        *pane,
                        (
                            modes.len(),
                            match modes.last()? {
                                PaneModeRequest::Clock => "clock-mode",
                                PaneModeRequest::Customize(_) => "options-mode",
                                PaneModeRequest::Switch(_) => "switch-mode",
                                PaneModeRequest::Panes(_) => "panes-mode",
                            },
                        ),
                    ))
                })
                .collect()
        })
    }

    fn pane_in_mode_count(&self, pane: PaneId) -> usize {
        pane_in_mode_count_from_parts(pane, self.pane_modes, self.clients, self.terminals)
    }
}

fn split_borrowed_format_hook_facts(
    inner: &mut ServerState,
    seed: CommandFormatSeed,
) -> (&mut MuxEngine, BorrowedFormatHookFacts<'_>) {
    let ServerState {
        engine,
        attachment:
            Attachment {
                attached,
                client_flags,
                session_last_attached,
                ..
            },
        clients,

        modes: Modes { pane_modes, .. },
        jobs: Jobs { pane_pipes, .. },
        buffers: Buffers { paste_buffers, .. },
        panes: PaneTables {
            terminal_geometries,
            ..
        },
        terminals,
        ..
    } = inner;
    let facts = BorrowedFormatHookFacts {
        terminals,
        pane_pipes,
        attached,
        clients,
        session_last_attached,

        pane_modes,
        paste_buffers,
        seed,
        derived: LazyDerivedFormatFacts::default(),
        client_fields: ClientFormatFields {
            clients,
            attached,
            client_flags,

            terminal_geometries,
            terminals,
        },
    };
    (engine, facts)
}

fn readonly_borrowed_format_hook_facts(
    inner: &ServerState,
    seed: CommandFormatSeed,
) -> BorrowedFormatHookFacts<'_> {
    BorrowedFormatHookFacts {
        client_fields: ClientFormatFields::from_inner(inner),
        paste_buffers: &inner.buffers.paste_buffers,
        terminals: &inner.terminals,
        pane_pipes: &inner.jobs.pane_pipes,
        attached: &inner.attachment.attached,
        clients: &inner.clients,
        session_last_attached: &inner.attachment.session_last_attached,
        pane_modes: &inner.modes.pane_modes,
        seed,
        derived: LazyDerivedFormatFacts::default(),
    }
}

fn borrowed_format_hook_facts(inner: &ServerState) -> BorrowedFormatHookFacts<'_> {
    readonly_borrowed_format_hook_facts(inner, CommandFormatSeed::default())
}

fn borrowed_format_hook_facts_for_client<'a>(
    inner: &'a ServerState,
    client: ClientId,
    context: &ExecutionContext,
) -> BorrowedFormatHookFacts<'a> {
    readonly_borrowed_format_hook_facts(inner, command_format_seed(inner, client, context))
}

fn command_format_seed(
    inner: &ServerState,
    client: ClientId,
    context: &ExecutionContext,
) -> CommandFormatSeed {
    let invoking = format_provenance_client(context, client);
    let format_client = if context.has_no_client() {
        hook_body_format_client(inner)
    } else {
        invoking.and_then(|client| current_format_client(inner, client))
    };
    CommandFormatSeed {
        client: format_client.and_then(|client| {
            client_attached_session(inner, client)
                .map(|session| client_format_facts(inner, client, session))
        }),
        invoking,
    }
}

#[cfg(test)]
thread_local! {
    static OWNED_FORMAT_FACT_BUILDS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Default)]
struct StatusFactSelection {
    full: bool,
    mux: bool,
    terminals: bool,
    pane_pipes: bool,
    session_attachments: bool,
    session_last_attached: bool,
    window_clients: bool,
    unseen_changes: bool,
    buffer: bool,
    copy_modes: bool,
    pane_modes: bool,
}

impl StatusFactSelection {
    fn from_references(references: &BTreeSet<String>) -> Self {
        #[cfg(test)]
        FORMAT_FACT_SELECTION_BUILDS.with(|count| count.set(count.get() + 1));
        let mut selection = Self::default();
        for name in references {
            match name.as_str() {
                "*" => {
                    selection.full = true;
                    return selection;
                }
                "pane_kind"
                | "browser_url"
                | "agent_state"
                | "agent_pending_permission"
                | "agent_parent" => {
                    selection.mux = true;
                }
                "pane_search_string"
                | "history_size"
                | "cursor_x"
                | "cursor_y"
                | "alternate_on"
                | "mouse_any_flag"
                | "cursor_flag"
                | "pane_last_command_status"
                | "pane_output_generation"
                | "pane_last_output_time"
                | "pane_last_prompt_time"
                | "pane_command_start_time"
                | "pane_command_end_time"
                | "pane_command_running"
                | "pane_command_duration"
                | "pane_command_status"
                | "pane_private_modes"
                | "pane_pb_progress"
                | "pane_pb_state"
                | "pane_status"
                | "pane_status_kind"
                | "pane_status_progress"
                | "pane_status_app"
                | "pane_status_title"
                | "pane_status_message"
                | "pane_status_raw_title"
                | "pane_status_raw_message"
                | "pane_status_reported" => selection.terminals = true,
                "pane_pipe" | "pane_pipe_pid" => selection.pane_pipes = true,
                "session_attached" | "session_attached_list" | "session_many_attached" => {
                    selection.session_attachments = true;
                }
                "session_last_attached" => selection.session_last_attached = true,
                "window_active_clients" | "window_active_clients_list" => {
                    selection.window_clients = true;
                }
                "pane_unseen_changes" => selection.unseen_changes = true,
                "buffer_created" | "buffer_full" | "buffer_name" | "buffer_sample"
                | "buffer_size" => {
                    selection.buffer = true;
                }
                "pane_in_mode" | "pane_mode" => {
                    selection.copy_modes = true;
                    selection.pane_modes = true;
                }
                name if name.starts_with("mode_") => {
                    selection.full = true;
                    return selection;
                }
                name if name.starts_with('@') => selection.mux = true,
                name if crate::status::COPY_MODE_CONTEXT_FORMATS.contains(&name) => {
                    selection.copy_modes = true;
                }
                _ => {}
            }
        }
        selection
    }
}

fn selected_status_format_facts(
    inner: &ServerState,
    context: &zz_mux::StatusContext,
    references: &BTreeSet<String>,
) -> FormatHookFacts {
    selected_status_format_facts_with_selection(
        inner,
        context,
        &StatusFactSelection::from_references(references),
    )
}

fn selected_status_format_facts_with_selection(
    inner: &ServerState,
    context: &zz_mux::StatusContext,
    selection: &StatusFactSelection,
) -> FormatHookFacts {
    use crate::status::FormatFactSource;

    if selection.full {
        return format_hook_facts(inner);
    }
    let source = readonly_borrowed_format_hook_facts(inner, CommandFormatSeed::default());
    let mut facts = FormatHookFacts::shared_empty();
    if selection.mux {
        facts.mux = Arc::new(inner.engine.format_facts());
    }
    if selection.terminals {
        facts.terminals = Arc::clone(&inner.terminals);
    }
    if selection.pane_pipes {
        facts.pane_pipes = Arc::new(source.pane_pipes().clone());
    }
    if selection.session_attachments {
        facts.session_attachments = Arc::new(source.session_attachments().clone());
    }
    if selection.session_last_attached {
        facts.session_last_attached = Arc::new(inner.attachment.session_last_attached.clone());
    }
    if selection.window_clients {
        facts.window_clients = Arc::new(source.window_clients(context).clone());
    }
    if selection.unseen_changes {
        facts.unseen_changes = Arc::new(source.unseen_changes().clone());
    }
    if selection.buffer {
        facts.buffer = source.buffer().cloned();
    }
    if selection.copy_modes {
        facts.copy_modes = Arc::new(source.copy_modes().clone());
    }
    if selection.pane_modes {
        facts.pane_modes = Arc::new(source.pane_modes().clone());
    }
    facts
}

fn format_hook_facts(inner: &ServerState) -> FormatHookFacts {
    #[cfg(test)]
    OWNED_FORMAT_FACT_BUILDS.with(|count| count.set(count.get() + 1));
    FormatHookFacts {
        mux: Arc::new(inner.engine.format_facts()),
        copy_modes: Arc::new(copy_mode_format_facts(inner)),
        pane_modes: Arc::new(pane_mode_format_facts(inner)),
        unseen_changes: Arc::new(unseen_change_panes(inner)),
        terminals: Arc::clone(&inner.terminals),
        pane_pipes: Arc::new(
            inner
                .jobs
                .pane_pipes
                .iter()
                .map(|(pane, pipe)| (*pane, pipe.pid))
                .collect(),
        ),
        session_attachments: Arc::new(
            inner
                .attachment
                .attached
                .iter()
                .map(|(session, clients)| {
                    let clients = clients
                        .iter()
                        .filter(|client| !inner.client(**client).is_some_and(|c| c.suspended))
                        .copied()
                        .collect::<Vec<_>>();
                    let names = clients
                        .iter()
                        .map(|client| client_format_name(inner, *client))
                        .collect::<Vec<_>>()
                        .join(",");
                    (*session, (clients.len(), names))
                })
                .collect(),
        ),
        window_clients: Arc::new(inner.attachment.attached.values().flatten().fold(
            BTreeMap::<WindowId, Vec<String>>::new(),
            |mut windows, client| {
                if let Some(window) = client_focused_window_for_attachment(inner, *client) {
                    windows
                        .entry(window)
                        .or_default()
                        .push(client_format_name(inner, *client));
                }
                windows
            },
        )),
        session_last_attached: Arc::new(inner.attachment.session_last_attached.clone()),
        clients: Arc::new(
            loop_clients_in_connect_order(&inner.attachment.attached, &inner.engine)
                .into_iter()
                .map(|(client, session)| {
                    client_format_facts(inner, client, session).loop_row(
                        inner
                            .client(client)
                            .and_then(|c| c.activity)
                            .unwrap_or_default(),
                    )
                })
                .collect(),
        ),
        buffer: inner
            .buffers
            .paste_buffers
            .iter()
            .find(|buffer| buffer.automatic)
            .map(buffer_format_facts),
        ..FormatHookFacts::default()
    }
}

fn loop_clients_in_connect_order(
    attached: &BTreeMap<SessionId, BTreeSet<ClientId>>,
    engine: &MuxEngine,
) -> Vec<(ClientId, SessionId)> {
    let mut clients = attached
        .iter()
        .filter(|(session, _)| engine.state.sessions.contains_key(session))
        .flat_map(|(session, clients)| clients.iter().map(move |client| (*client, *session)))
        .collect::<Vec<_>>();
    clients.sort_unstable();
    clients
}

fn hook_body_format_client(inner: &ServerState) -> Option<ClientId> {
    inner
        .engine
        .state
        .most_recent_context()
        .and_then(|(session, _, _)| best_client_on_session(inner, session))
        .or_else(|| best_attached_client(inner))
}

fn format_provenance_client(
    context: &ExecutionContext,
    invoking_client: ClientId,
) -> Option<ClientId> {
    context
        .replay_client()
        .or_else(|| context.control_command_target().map(|(client, _)| client))
        .or((invoking_client != ClientId(u64::MAX)).then_some(invoking_client))
}

#[cfg(test)]
#[path = "daemon/format_refresh_tests.rs"]
mod format_refresh_tests;

fn current_format_client(inner: &ServerState, invoking_client: ClientId) -> Option<ClientId> {
    if client_attached_session(inner, invoking_client).is_some() {
        return Some(invoking_client);
    }
    if let Some(session) = inner
        .client(invoking_client)
        .and_then(|c| c.origin.as_ref())
        .and_then(|pane| inner.engine.state.window_for_pane(*pane))
        .and_then(|window| inner.engine.state.windows.get(&window))
        .map(|window| window.session)
    {
        return best_client_on_session(inner, session).or_else(|| best_attached_client(inner));
    }
    inner
        .engine
        .state
        .most_recent_context()
        .and_then(|(session, _, _)| best_client_on_session(inner, session))
        .or_else(|| best_attached_client(inner))
}

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

#[derive(Clone)]
enum InsertedCommandSource {
    Commands(Vec<CommandInvocation>),
    Shell(String),
    String(String),
    Block(RawText),
    Hooks(Box<HookQueueSource>),
    Events(Box<EventQueueSource>),
}

#[cfg(test)]
#[path = "daemon/alias_queue_tests.rs"]
mod alias_queue_tests;

enum InsertedFrameExecution<'a> {
    Borrowed(&'a CommandQueueExecution),
    Owned(Box<CommandQueueExecution>),
}

impl std::ops::Deref for InsertedFrameExecution<'_> {
    type Target = CommandQueueExecution;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(execution) => execution,
            Self::Owned(execution) => execution,
        }
    }
}

trait FrameExecution: std::ops::Deref<Target = CommandQueueExecution> {
    fn owned(execution: CommandQueueExecution) -> Self;
}

impl FrameExecution for InsertedFrameExecution<'_> {
    fn owned(execution: CommandQueueExecution) -> Self {
        Self::Owned(Box::new(execution))
    }
}

impl FrameExecution for Box<CommandQueueExecution> {
    fn owned(execution: CommandQueueExecution) -> Self {
        Box::new(execution)
    }
}

struct InsertedQueueFrame<E: FrameExecution> {
    request_root: bool,
    request_result: Option<Result<Execution, DaemonError>>,
    wait_boundary: Option<(InsertedCommandBoundary, InsertedCommandStep)>,
    execution: E,
    context: ExecutionContext,
    commands: std::vec::IntoIter<CommandInvocation>,
    prepared: bool,
    control_target: Option<(ClientId, u8)>,
    mux_source: MuxOptionSource,
    alias_terminal: Option<ClientTerminal>,
    stdin: Option<RawText>,
    carried_a_stream: bool,
    label: String,
    result: InsertedCommandResult,
    stdout_claim: StdoutClaim,
    first_error: Option<DaemonError>,
    failed_group: Option<InsertedPhysicalGroup>,
    boundary: Option<(
        InsertedCommandBoundary,
        Box<InsertedQueueChild>,
        cmdq::ContinuationToken,
    )>,
    terminal_error: Option<DaemonError>,
    parked_boundary: Option<InsertedCommandBoundary>,
    hook: Option<HookQueueFrame<E>>,
    events: Option<EventQueueFrame>,
}

fn inserted_frame_mode(
    execution: &CommandQueueExecution,
    terminal: Option<ClientTerminal>,
) -> InsertedCommandMode<'_> {
    match terminal {
        Some(client_terminal) => InsertedCommandMode::CommandAlias {
            client_terminal,
            queue_execution: execution,
            stdin: None,
        },
        None => InsertedCommandMode::Standard(execution),
    }
}

#[derive(PartialEq, Eq)]
enum InsertedPhysicalGroup {
    Unlocated,
    Source(String, u32),
}

struct InsertedCommandBoundary {
    command: CommandInvocation,
    group: InsertedPhysicalGroup,
    callback_failures_start: usize,
    command_control_target: Option<(ClientId, u8)>,
    stdout_sequence: usize,
}

enum InsertedFrameAction {
    Continue,
    Finish(Option<DaemonError>),
}

type InsertedCommandStep = (
    Result<Execution, DaemonError>,
    Option<String>,
    bool,
    Option<CallbackParseFailure>,
    bool,
);

struct InsertedQueueChild {
    context: ExecutionContext,
    source: InsertedCommandSource,
    label: String,
    control_target: Option<(ClientId, u8)>,
    mux_source: MuxOptionSource,
    kind: InsertedQueueChildKind,
    leaf: Option<InsertedLeafContinuation>,
    guard: Option<InsertedControlGuard>,
    leaf_name: Option<String>,
}

enum InsertedQueueChildKind {
    Alias {
        client_terminal: ClientTerminal,
        stdin: Option<RawText>,
    },
    Foreground(Option<RunShellRoute>),
    Hook(Option<HookLeafContinuation>),
    Event {
        draining: bool,
    },
}

#[derive(Clone)]
struct EventQueueSource {
    events: RefCell<Vec<PendingHookEvent>>,
    publish_control: bool,
    shutdown_already_blocked: bool,
}

struct EventQueueFrame {
    events: VecDeque<PendingHookEvent>,
    publish_control: bool,
    shutdown_already_blocked: bool,
    notifications: Vec<(EventPayload, Option<ClientId>, bool)>,
}

#[derive(Clone)]
struct HookQueueSource {
    commands: RefCell<Vec<Vec<CommandInvocation>>>,
    variables: BTreeMap<String, String>,
    skip_resolution_errors: bool,
    initial_draining: bool,
    replaying: bool,
}

struct HookQueueFrame<E: FrameExecution> {
    parent: E,
    variables: BTreeMap<String, String>,
    skip_resolution_errors: bool,
    replaying: bool,
    initial_draining: bool,
    groups: VecDeque<Vec<CommandInvocation>>,
    context: ExecutionContext,
}

struct HookLeafContinuation {
    result: Result<Execution, DaemonError>,
    pane_exit_code: u8,
}

struct InsertedLeafContinuation {
    original_context: ExecutionContext,
    previous_client_terminal: ClientTerminal,
    no_hooks: bool,
    name: String,
    command: Option<CommandInvocation>,
    pane_exit_status: Option<Arc<AtomicU8>>,
}

struct InsertedControlGuard {
    target: (ClientId, u8),
    aggregate_forced_shutdown: bool,
    deferred_shutdown_before: DeferredShutdown,
    reported_failure_before: bool,
    early_shell_guard: bool,
    direct_command_prepare_error: bool,
    source_command: bool,
    alias_group: bool,
    routed_name: Option<String>,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum CallbackGroupAction {
    Continue,
    Fail,
    Handled,
}

impl CallbackGroupAction {
    fn for_command(name: &str) -> Option<Self> {
        match name {
            "if-shell" => Some(Self::Fail),
            "run-shell" => Some(Self::Continue),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct CallbackParseFailure {
    message: String,
    group_action: CallbackGroupAction,
    owner: CallbackFailureOwner,
    control_event: Option<Arc<()>>,
    depth: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CallbackFailureOwner {
    Source,
    RuntimeCommand,
    Hook,
    CommandErrorHook,
}

impl CallbackFailureOwner {
    fn is_replay_callback(self) -> bool {
        matches!(self, Self::Source | Self::RuntimeCommand)
    }
}

impl CallbackParseFailure {
    fn for_command(
        message: String,
        name: &str,
        control_event: Option<Arc<()>>,
        depth: usize,
    ) -> Option<Self> {
        Some(Self {
            message,
            group_action: CallbackGroupAction::for_command(name)?,
            owner: CallbackFailureOwner::Source,
            control_event,
            depth,
        })
    }

    fn runtime_command(message: String, label: &str) -> Self {
        let name = label
            .strip_prefix('<')
            .and_then(|label| label.strip_suffix('>'))
            .unwrap_or(label)
            .split_ascii_whitespace()
            .next()
            .unwrap_or(label);
        Self {
            message,
            group_action: CallbackGroupAction::for_command(name)
                .unwrap_or(CallbackGroupAction::Handled),
            owner: CallbackFailureOwner::RuntimeCommand,
            control_event: None,
            depth: 0,
        }
    }
}

fn select_callback_group_action(
    failures: &[CallbackParseFailure],
    routed_name: &str,
    terminal_depth: usize,
) -> Option<CallbackGroupAction> {
    let mut continuing = false;
    for failure in failures.iter().filter(|failure| {
        failure.owner.is_replay_callback()
            && failure.group_action != CallbackGroupAction::Handled
            && failure.depth == 1
    }) {
        match failure.group_action {
            CallbackGroupAction::Fail => return Some(CallbackGroupAction::Fail),
            CallbackGroupAction::Continue => continuing = true,
            CallbackGroupAction::Handled => unreachable!(),
        }
    }
    continuing
        .then_some(CallbackGroupAction::Continue)
        .or_else(|| {
            (terminal_depth == 1)
                .then(|| CallbackGroupAction::for_command(routed_name))
                .flatten()
        })
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

#[derive(Clone, Copy)]
enum InsertedCommandMode<'a> {
    Standard(&'a CommandQueueExecution),
    CommandAlias {
        client_terminal: ClientTerminal,
        queue_execution: &'a CommandQueueExecution,
        stdin: Option<&'a RawText>,
    },
}

impl<'a> InsertedCommandMode<'a> {
    fn queue_execution(self) -> &'a CommandQueueExecution {
        match self {
            Self::Standard(queue_execution)
            | Self::CommandAlias {
                queue_execution, ..
            } => queue_execution,
        }
    }
}

#[derive(Default)]
struct InsertedCommandResult {
    output: RawText,
    exit_code: u8,
    yielded: bool,
}

#[derive(Clone, Copy)]
struct RunShellRoute {
    client: ClientId,
    kind: ClientKind,
    control_target: Option<ClientId>,
    foreground: bool,
    target_requested: bool,
    target: Option<PaneId>,
}

struct ParsedRunShellArgs {
    background: bool,
    command_mode: bool,
    show_stderr: bool,
    cwd: Option<String>,
    delay: Option<Duration>,
    positive_delay: bool,
    target: Option<String>,
    positional: Vec<String>,
    positional_start: usize,
}

struct ParsedIfShellArgs {
    background: bool,
    format: bool,
    target: Option<String>,
    positional: Vec<String>,
    positional_start: usize,
}

#[cfg(test)]
const RUN_SHELL_POSITION_CONTEXT_PATTERN: &str = "N";

fn parse_run_shell_args(args: &[RawText]) -> Result<ParsedRunShellArgs, ServerError> {
    let parsed =
        parse_buffer_command_args("run-shell", args, &['c', 'd', 's', 't'], &['b', 'C', 'E'])?;
    let delay_value = parsed.value('d');
    let delay = delay_value.map(parse_shell_delay).transpose()?;
    let positive_delay = delay_value
        .and_then(parse_shell_delay_seconds)
        .is_some_and(|seconds| seconds > 0.0);
    let positional_start = args.len().saturating_sub(parsed.positional.len());
    Ok(ParsedRunShellArgs {
        background: parsed.has('b'),
        command_mode: parsed.has('C'),
        show_stderr: parsed.has('E'),
        cwd: parsed.value('c').map(str::to_owned),
        delay,
        positive_delay,
        target: parsed.value('t').map(str::to_owned),
        positional: parsed.positional,
        positional_start,
    })
}

fn parse_if_shell_args(args: &[RawText]) -> Result<ParsedIfShellArgs, ServerError> {
    let parsed = parse_buffer_command_args("if-shell", args, &['t'], &['b', 'F'])?;
    let positional_start = args.len().saturating_sub(parsed.positional.len());
    Ok(ParsedIfShellArgs {
        background: parsed.has('b'),
        format: parsed.has('F'),
        target: parsed.value('t').map(str::to_owned),
        positional: parsed.positional,
        positional_start,
    })
}

fn parse_shell_delay(value: &str) -> Result<Duration, ServerError> {
    let seconds = parse_shell_delay_seconds(value)
        .ok_or_else(|| ServerError::InvalidCommand(format!("invalid delay time: {value}")))?;
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| ServerError::InvalidCommand(format!("invalid delay time: {value}")))
}

fn parse_shell_delay_seconds(value: &str) -> Option<f64> {
    if value.is_empty() {
        return Some(0.0);
    }
    let value = value.trim_start_matches(char::is_whitespace);
    if value.is_empty() {
        return Some(0.0);
    }
    let (negative, unsigned) = value
        .strip_prefix('-')
        .map_or((false, value), |value| (true, value));
    let unsigned = unsigned.strip_prefix('+').unwrap_or(unsigned);
    let seconds = if let Some(hex) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()? as f64
    } else {
        value.parse::<f64>().ok()?
    };
    let seconds = if negative { -seconds } else { seconds };
    (seconds.is_finite() && seconds >= 0.0).then_some(seconds)
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

fn append_inserted_output(output: &mut RawText, addition: impl AsRef<[u8]>) {
    let addition = addition.as_ref();
    if addition.is_empty() {
        return;
    }
    if !output.as_bytes().is_empty() && !output.as_bytes().ends_with(b"\n") {
        output.push_bytes(b"\n");
    }
    output.push_bytes(addition);
}

fn prepend_command_output(mut output: RawText, error: DaemonError) -> DaemonError {
    if output.as_bytes().is_empty() {
        return error;
    }
    match error {
        DaemonError::CommandExit {
            output: error_output,
            exit_code,
        } => {
            append_inserted_output(&mut output, &error_output);
            DaemonError::CommandExit { output, exit_code }
        }
        DaemonError::ReportedCommandExit {
            output: error_output,
            exit_code,
        } => {
            append_inserted_output(&mut output, &error_output);
            DaemonError::ReportedCommandExit { output, exit_code }
        }
        DaemonError::CommandFailed {
            output: error_output,
            error,
        } => {
            append_inserted_output(&mut output, &error_output);
            DaemonError::CommandFailed { output, error }
        }
        error => DaemonError::CommandFailed {
            output,
            error: Box::new(error),
        },
    }
}

fn discard_all_command_output(error: DaemonError) -> DaemonError {
    match error {
        DaemonError::CommandExit { exit_code, .. } => DaemonError::CommandExit {
            output: RawText::default(),
            exit_code,
        },
        DaemonError::ReportedCommandExit { exit_code, .. } => DaemonError::ReportedCommandExit {
            output: RawText::default(),
            exit_code,
        },
        DaemonError::CommandFailed { error, .. } => discard_all_command_output(*error),
        error => error,
    }
}

fn inserted_execution(
    client: ClientId,
    kind: ClientKind,
    result: InsertedCommandResult,
) -> Result<Execution, DaemonError> {
    if matches!(kind, ClientKind::Command | ClientKind::Control)
        && client != ClientId(u64::MAX)
        && result.exit_code != 0
    {
        Err(DaemonError::CommandExit {
            output: result.output,
            exit_code: result.exit_code,
        })
    } else {
        Ok(Execution {
            output: result.output,
            effects: Vec::new(),
        })
    }
}

fn default_job_working_directory() -> PathBuf {
    home_directory().unwrap_or_else(|| PathBuf::from("/"))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandStdinSink {
    Argument { binary: bool },
    Config,
    ConfigReplay,
    PaneInput,
}

impl CommandStdinSink {
    #[must_use]
    pub const fn accepts_binary(self) -> bool {
        match self {
            Self::Argument { binary } => binary,
            Self::Config | Self::PaneInput | Self::ConfigReplay => true,
        }
    }

    #[must_use]
    pub const fn is_argument(self) -> bool {
        matches!(self, Self::Argument { .. })
    }
}

#[must_use]
pub fn command_stdin_sink(canonical_name: &str, args: &[RawText]) -> Option<CommandStdinSink> {
    match canonical_name {
        "agent-send" => {
            agent_send_reads_stdin(args).then_some(CommandStdinSink::Argument { binary: false })
        }
        "send-text" => {
            send_text_reads_stdin(args).then_some(CommandStdinSink::Argument { binary: false })
        }
        "html" => artifacts::html_reads_stdin(args)
            .then_some(CommandStdinSink::Argument { binary: false }),
        "load-buffer" => {
            load_buffer_reads_stdin(args).then_some(CommandStdinSink::Argument { binary: true })
        }
        "source-file" => Some(if source_file_reads_stdin(args) {
            CommandStdinSink::Config
        } else {
            CommandStdinSink::ConfigReplay
        }),
        "display-message" | "split-window" | "new-pane" => (args
            .iter()
            .any(|argument| argument.as_bytes().contains(&b'I'))
            && command_has_flag(canonical_name, args, "-I")
            && !(canonical_name == "display-message"
                && command_has_flag(canonical_name, args, "-j")))
        .then_some(CommandStdinSink::PaneInput),
        _ => None,
    }
}

fn stdin_payload_has_argument_boundary(canonical_name: &str, arguments: &[RawText]) -> bool {
    if canonical_name == "load-buffer" {
        return true;
    }
    let Some(spec) = zz_protocol::catalog_command_spec(canonical_name) else {
        return false;
    };
    let mut index = 0;
    while let Some(argument) = arguments.get(index) {
        if argument == "--" {
            return true;
        }
        if !argument.starts_with('-') || argument == "-" {
            return false;
        }
        let consumes_next = spec
            .option(argument)
            .is_some_and(|option| option.value.is_some());
        index += if consumes_next { 2 } else { 1 };
    }
    false
}

pub fn append_stdin_payload(
    canonical_name: &str,
    arguments: &mut Vec<RawText>,
    payload: impl Into<RawText>,
) {
    if !stdin_payload_has_argument_boundary(canonical_name, arguments) {
        arguments.push("--".into());
    }
    arguments.push(payload.into());
}

fn source_file_reads_stdin(args: &[RawText]) -> bool {
    zz_protocol::command_spec("source-file").is_some_and(|spec| {
        zz_protocol::parse_tmux_options(spec, args)
            .is_ok_and(|parsed| parsed.positionals.iter().any(|path| path == "-"))
    })
}

fn command_has_flag(canonical_name: &str, args: &[RawText], flag: &str) -> bool {
    zz_protocol::command_spec(canonical_name).is_some_and(|spec| {
        zz_protocol::parse_tmux_options(spec, args).is_ok_and(|parsed| {
            parsed.options.iter().any(
                |option| matches!(option, zz_protocol::TmuxOption::Flag(name) if *name == flag),
            )
        })
    })
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ContextReference {
    path: String,
    start: Option<u32>,
    end: Option<u32>,
}

impl ContextReference {
    fn parse(value: &str) -> Result<Self, ServerError> {
        let invalid = |message: &str| ServerError::InvalidCommand(message.to_owned());
        let (path, range) = match value.rsplit_once(':') {
            Some((path, range)) if !path.is_empty() && parse_line_range(range).is_some() => {
                (path, parse_line_range(range))
            }
            _ => (value, None),
        };
        if path.is_empty() || path.len() > MAX_CONTEXT_PATH_BYTES {
            return Err(invalid("--context path must be 1..=4096 bytes"));
        }
        if path.chars().any(char::is_control) {
            return Err(invalid(
                "--context path must not contain control characters",
            ));
        }
        let (start, end) = range.unwrap_or((None, None));
        if let (Some(start), Some(end)) = (start, end)
            && start > end
        {
            return Err(invalid("--context line range must not run backwards"));
        }
        Ok(Self {
            path: path.to_owned(),
            start,
            end,
        })
    }

    fn header(&self) -> String {
        match (self.start, self.end) {
            (Some(start), Some(end)) => format!("{}:{start}-{end}", self.path),
            (Some(start), None) => format!("{}:{start}", self.path),
            _ => self.path.clone(),
        }
    }
}

fn parse_line_range(value: &str) -> Option<(Option<u32>, Option<u32>)> {
    if let Some((start, end)) = value.split_once('-') {
        return Some((Some(start.parse().ok()?), Some(end.parse().ok()?)));
    }
    Some((Some(value.parse().ok()?), None))
}

pub(crate) use zz_protocol::agent_stream::AgentBlockPolicy;

#[derive(Clone, Debug, Default)]
struct ParsedAgentSend {
    on_block: AgentBlockPolicy,
    on_block_set: bool,
    json: bool,
    final_only: bool,
    target: Option<String>,
    submit: bool,
    wait: bool,
    notify: bool,
    timeout: Option<Duration>,
    context: Option<ContextReference>,
    relay: Option<String>,
    text: Vec<String>,
}

fn pane_wait_exit_code(terminal: &TerminalSession, status: &zz_terminal::SessionStatus) -> u8 {
    if let Some(completion) = terminal.completion() {
        return completion.signal.map_or_else(
            || u8::try_from(completion.code).unwrap_or(u8::MAX),
            |signal| signal.saturating_add(128),
        );
    }
    match status {
        zz_terminal::SessionStatus::Exited(status) => u8::try_from(status.code).unwrap_or(u8::MAX),
        _ => 0,
    }
}

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

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn echo_tail(text: &str) -> String {
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    let collapsed = collapse_whitespace(line);
    let start = collapsed
        .char_indices()
        .rev()
        .nth(SEND_TEXT_TAIL_CHARS.saturating_sub(1))
        .map_or(0, |(index, _)| index);
    collapsed[start..].to_owned()
}

#[derive(Debug)]
struct ParsedSendText {
    target: Option<String>,
    no_enter: bool,
    timeout: Duration,
    text: Vec<String>,
}

impl ParsedSendText {
    fn payload(&self) -> Result<String, ServerError> {
        let text = self.text.join(" ");
        let text = text.trim_end_matches(['\n', '\r']);
        check_plain_text("send-text", text)?;
        Ok(text.to_owned())
    }
}

#[must_use]
pub fn send_text_reads_stdin(args: &[RawText]) -> bool {
    parse_send_text_args(args).is_ok_and(|parsed| parsed.text.is_empty())
}

#[derive(Debug)]
enum PaneWaitCondition {
    Exit,
    Idle(Duration),
    Until(String),
    Regex(regex::Regex),
}

#[derive(Debug)]
struct ParsedWaitPane {
    target: Option<String>,
    condition: PaneWaitCondition,
    timeout: Duration,
    tail: Option<usize>,
}

#[derive(Debug)]
struct ParsedRunPane {
    target: Option<String>,
    timeout: Duration,
    command: String,
}

fn parse_pane_timeout(verb: &str, value: &str) -> Result<Duration, ServerError> {
    let invalid = || {
        ServerError::CommandParse(format!(
            "{verb} --timeout needs a nonnegative number of seconds"
        ))
    };
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid());
    }
    Duration::try_from_secs_f64(value.parse().map_err(|_| invalid())?).map_err(|_| invalid())
}

const RUN_PANE_LITERAL: &str = " _./,:=+@%~-";

fn run_pane_line(marker: &str, eval: &str, command: &str) -> String {
    let mut escaped = String::with_capacity(command.len());
    for character in command.chars() {
        if character.is_ascii_alphanumeric()
            || !character.is_ascii()
            || RUN_PANE_LITERAL.contains(character)
        {
            escaped.push(character);
        } else {
            let _ = write!(escaped, "\\0{:03o}", u32::from(character));
        }
    }
    format!(
        " printf '\\n%s\\n' '{marker}-BEGIN'; {eval} \"$(printf '%b' '{escaped}')\"; printf \
         '\\n{marker}-RC=%d=END\\n' $?"
    )
}

fn run_pane_command(escaped: &str) -> Option<String> {
    let mut command = String::with_capacity(escaped.len());
    let mut rest = escaped;
    while let Some(index) = rest.find("\\0") {
        command.push_str(&rest[..index]);
        let digits = rest.get(index + 2..index + 5)?;
        command.push(char::from(u8::from_str_radix(digits, 8).ok()?));
        rest = &rest[index + 5..];
    }
    command.push_str(rest);
    Some(command)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunStart {
    Marked,
    Dropped,
    Joined,
}

fn run_pane_code(text: &str, prefix: &str) -> Option<(usize, u8)> {
    text.match_indices(prefix).find_map(|(index, _)| {
        let rest = &text[index + prefix.len()..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if !rest[digits..].starts_with("=END") {
            return None;
        }
        Some((index, rest[..digits].parse().ok()?))
    })
}

fn run_pane_result<'a>(
    screen: &'a str,
    marker: &str,
    collecting: bool,
) -> (Option<&'a str>, Option<u8>, RunStart) {
    let begin = format!("{marker}-BEGIN");
    let typed = format!("'{begin}'");
    let prefix = format!("{marker}-RC=");
    let echoed = format!("{prefix}%d=END\\n' $?");
    let lines = screen
        .split_inclusive('\n')
        .rev()
        .take(RUN_PANE_CAPTURE_LINES)
        .collect::<Vec<_>>();
    let mut offset = screen.len() - lines.iter().map(|line| line.len()).sum::<usize>();
    let capture_start = offset;
    let mut start = collecting.then_some(offset);
    let mut inline = false;
    let mut saw_begin = false;
    let mut echo_end = None;
    for line in lines.into_iter().rev() {
        let text = line.strip_suffix('\n').unwrap_or(line);
        let code = run_pane_code(text, &prefix);
        let before = &text[..code.map_or(text.len(), |(index, _)| index)];
        let boundary = |after: usize| {
            if text[after..].trim().is_empty() {
                (offset + line.len(), false)
            } else {
                (offset + after, true)
            }
        };
        if let Some(index) = before
            .match_indices(&begin)
            .map(|(index, _)| index)
            .filter(|index| !before[..*index].ends_with('\''))
            .last()
        {
            let (at, within) = boundary(index + begin.len());
            start = Some(at);
            inline = within;
            saw_begin = true;
        } else if before.contains(&typed) {
            let (at, within) = boundary(
                before
                    .rfind(&echoed)
                    .map_or(before.len(), |index| index + echoed.len()),
            );
            echo_end = Some(at);
            inline = within;
        }
        if let Some((index, code)) = code {
            let begin = if saw_begin {
                RunStart::Marked
            } else if start.is_none() && echo_end.is_some() {
                RunStart::Joined
            } else {
                RunStart::Dropped
            };
            let end = if text[..index].trim().is_empty() {
                offset
            } else {
                offset + index
            };
            let segment = &screen[start.or(echo_end).unwrap_or(capture_start).min(end)..end];
            let segment = if end > offset {
                segment.trim_end()
            } else {
                segment.strip_suffix('\n').unwrap_or(segment)
            };
            return (
                Some(if inline {
                    segment.trim_start()
                } else {
                    segment
                }),
                Some(code),
                begin,
            );
        }
        offset += line.len();
    }
    let partial = start.map(|start| {
        let rest = &screen[start..];
        let kept = rest.trim_end_matches('\n').len();
        let rest = &rest[..if kept == 0 {
            0
        } else {
            (kept + 1).min(rest.len())
        }];
        if inline { rest.trim_start() } else { rest }
    });
    (partial, None, RunStart::Marked)
}

fn parse_send_text_args(args: &[RawText]) -> Result<ParsedSendText, ServerError> {
    let mut parsed = ParsedSendText {
        target: None,
        no_enter: false,
        timeout: SEND_TEXT_TIMEOUT,
        text: Vec::new(),
    };
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            parsed.text.extend(
                args[index.saturating_add(1)..]
                    .iter()
                    .map(ToString::to_string),
            );
            break;
        }
        if argument == "--no-enter" {
            parsed.no_enter = true;
            index += 1;
            continue;
        }
        if let Some(value) = option_value("send-text", args, index, &["--timeout"])? {
            parsed.timeout = parse_pane_timeout("send-text", &value.value)?;
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("send-text", args, index, &["-t"])? {
            parsed.target = Some(value.value);
            index += value.consumed;
            continue;
        }
        if argument.starts_with('-') && argument != "-" {
            return Err(ServerError::CommandParse(format!(
                "unsupported command: send-text {argument}"
            )));
        }
        parsed.text.push(argument.to_string());
        index += 1;
    }
    Ok(parsed)
}

#[derive(serde::Serialize)]
struct AgentReplyEnvelope<'a> {
    pane: String,
    stop_reason: &'a str,
    duration_ms: u128,
    tool_calls: u32,
    permissions: AgentReplyPermissions,
    truncated: bool,
    final_text: &'a str,
    transcript: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    permission: Option<&'a serde_json::Value>,
}

#[derive(Default, serde::Serialize)]
struct AgentReplyPermissions {
    requested: u32,
    allowed: u32,
    denied: u32,
}

fn agent_terminal_reply_output(
    pane: PaneId,
    text: &str,
    duration: Duration,
    stop_reason: &str,
    parsed: &ParsedAgentSend,
) -> String {
    if !parsed.json {
        return text.to_owned();
    }
    serde_json::to_string(&AgentReplyEnvelope {
        pane: pane.to_string(),
        stop_reason,
        duration_ms: duration.as_millis(),
        tool_calls: 0,
        permissions: AgentReplyPermissions::default(),
        truncated: false,
        final_text: text,
        transcript: text,
        permission: None,
    })
    .expect("turn reply JSON")
}

#[cfg(feature = "agent")]
fn agent_permission_json(permission: &zz_protocol::AgentPermissionWire) -> serde_json::Value {
    let payload: serde_json::Value =
        serde_json::from_str(&permission.payload).expect("permission payload JSON");
    serde_json::json!({ "request_id": permission.request_id, "tool_call": payload["toolCall"], "options": payload["options"] })
}

impl ParsedAgentSend {
    fn wait_only(&self) -> bool {
        self.wait && self.context.is_none() && self.text.iter().all(|text| text.trim().is_empty())
    }

    fn wait_timeout(&self) -> Option<Duration> {
        let timeout = self.timeout.unwrap_or(AGENT_SEND_WAIT_TIMEOUT);
        (!timeout.is_zero()).then_some(timeout)
    }

    fn payload(&self) -> Result<String, ServerError> {
        let text = self.text.join(" ");
        let text = text.trim_end_matches(['\n', '\r']);
        check_plain_text("agent-send", text)?;
        let payload = match &self.context {
            Some(reference) => fenced_block(&reference.header(), text),
            None => text.to_owned(),
        };
        if payload.len() > MAX_AGENT_SEND_BYTES {
            return Err(ServerError::InvalidCommand(format!(
                "agent-send payload is {} bytes with its context header; the limit is \
                 {MAX_AGENT_SEND_BYTES}",
                payload.len()
            )));
        }
        Ok(payload)
    }
}

fn fenced_block(header: &str, text: &str) -> String {
    let widest = text
        .split('\n')
        .flat_map(|line| line.split(|character| character != '`'))
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(widest.saturating_add(1).max(3));
    format!("{header}\n{fence}\n{text}\n{fence}")
}

#[must_use]
pub fn agent_send_reads_stdin(args: &[RawText]) -> bool {
    parse_agent_send_args(args).is_ok_and(|parsed| parsed.text.is_empty())
}

#[cfg(feature = "agent")]
const MAX_AGENT_NOTIFY_BYTES: usize = 64 * 1024;

#[cfg(feature = "agent")]
fn reply_tail(text: &str) -> String {
    let mut start = text.len().saturating_sub(MAX_AGENT_NOTIFY_BYTES);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].trim().to_owned()
}

#[cfg(feature = "agent")]
#[derive(Clone, Debug)]
struct ChatDelivery {
    from: PaneId,
    request: Option<u64>,
    text: String,
}

fn parse_agent_send_args(args: &[RawText]) -> Result<ParsedAgentSend, ServerError> {
    let mut parsed = ParsedAgentSend::default();
    let mut on_block_set = false;
    let mut progress = false;
    let mut explicit_target = false;
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            parsed.text.extend(
                args[index.saturating_add(1)..]
                    .iter()
                    .map(ToString::to_string),
            );
            break;
        }
        if argument == "--submit" {
            parsed.submit = true;
            index += 1;
            continue;
        }
        if argument == "--progress" {
            progress = true;
            index += 1;
            continue;
        }
        if argument == "--wait" {
            parsed.submit = true;
            parsed.wait = true;
            index += 1;
            continue;
        }
        if argument == "--notify" {
            parsed.submit = true;
            parsed.notify = true;
            index += 1;
            continue;
        }
        if argument == "--json" || argument == "--final" {
            parsed.json |= argument == "--json";
            parsed.final_only |= argument == "--final";
            index += 1;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["--on-block"])? {
            parsed.on_block = match value.value.as_str() {
                "wait" => AgentBlockPolicy::Wait,
                "fail" => AgentBlockPolicy::Fail,
                "allow" => AgentBlockPolicy::Allow,
                "deny" => AgentBlockPolicy::Deny,
                _ => {
                    return Err(ServerError::CommandParse(
                        "agent-send --on-block needs wait, fail, allow, or deny".to_owned(),
                    ));
                }
            };
            on_block_set = true;
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["--timeout"])? {
            parsed.timeout = Some(parse_pane_timeout("agent-send", &value.value)?);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["-t"])? {
            explicit_target = true;
            parsed.target = Some(value.value);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["--context"])? {
            parsed.context = Some(ContextReference::parse(&value.value)?);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("agent-send", args, index, &["--relay"])? {
            parsed.relay = Some(value.value);
            index += value.consumed;
            continue;
        }
        if argument.starts_with('-') && argument != "-" {
            return Err(ServerError::CommandParse(format!(
                "unsupported command: agent-send {argument}"
            )));
        }
        parsed.text.push(argument.to_string());
        index += 1;
    }
    if (parsed.json || parsed.final_only) && !parsed.wait {
        return Err(ServerError::CommandParse(
            "agent-send --json and --final need --wait".to_owned(),
        ));
    }
    if parsed.json && parsed.final_only {
        return Err(ServerError::CommandParse(
            "agent-send --json and --final cannot be combined".to_owned(),
        ));
    }
    if parsed.notify && parsed.wait {
        return Err(ServerError::CommandParse(
            "agent-send --notify and --wait cannot be combined".to_owned(),
        ));
    }
    if progress
        && (!parsed.wait
            || !explicit_target
            || !parsed.target.as_deref().is_some_and(|target| {
                target
                    .strip_prefix('%')
                    .is_some_and(|id| !id.is_empty() && id.bytes().all(|ch| ch.is_ascii_digit()))
            }))
    {
        return Err(ServerError::NativeCommandParse(
            "agent-send: --progress requires --wait and an explicit -t %N".to_owned(),
        ));
    }
    parsed.on_block_set = on_block_set;
    if on_block_set && !parsed.wait {
        return Err(ServerError::CommandParse(
            "agent-send --on-block needs --wait".to_owned(),
        ));
    }
    if parsed.timeout.is_some() && !parsed.wait {
        return Err(ServerError::CommandParse(
            "agent-send --timeout needs --wait".to_owned(),
        ));
    }
    Ok(parsed)
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

pub fn workspace_tools(args: &[RawText]) -> Result<String, ServerError> {
    let section = match args {
        [] => return Ok(WORKSPACE_TOOLS.to_owned()),
        [section] if section == "agent" => WORKSPACE_TOOLS_AGENT,
        [section] if section == "terminal" => WORKSPACE_TOOLS_TERMINAL,
        [section] if section == "browser" => WORKSPACE_TOOLS_BROWSER,
        [section] if section == "advanced" => WORKSPACE_TOOLS_ADVANCED,
        [flag] if flag == "--skill" => {
            return Ok(format!(
                "{WORKSPACE_TOOLS_FRONTMATTER}{WORKSPACE_TOOLS_HEAD}{WORKSPACE_TOOLS_AGENT}{WORKSPACE_TOOLS_ETIQUETTE}"
            ));
        }
        [flag] if WORKSPACE_TOOLS_LOCAL_FLAGS.contains(&flag.as_str()) => {
            return Err(ServerError::CommandParse(format!(
                "tools {flag} runs in the local zz CLI"
            )));
        }
        _ => {
            return Err(ServerError::CommandParse(
                "usage: tools [agent|terminal|browser|advanced | --skill | --context | --install-hooks | --remove-hooks]".to_owned(),
            ));
        }
    };
    Ok(format!("{WORKSPACE_TOOLS_SECTION_HEAD}{section}"))
}

const WORKSPACE_TOOLS_SECTION_HEAD: &str =
    "Run `zz tools` for targets, remote hosts, exit codes and every section.\n\n";

const WORKSPACE_TOOLS_FRONTMATTER: &str = r"---
name: zz-workspace
description: Drive the surrounding zz workspace from inside a zz pane using the `zz` CLI to send text or piped output to an agent running in another pane, read a terminal pane's scrollback, screenshot a browser pane, navigate a browser, and route a failing command's output to an agent. Use whenever you are running inside zz (the `ZZ_PANE` environment variable is set) and the task involves another pane, a terminal's output, a browser page, or another machine.
---

";

pub const WORKSPACE_STUB_SKILL: &str = r#"---
name: zz-workspace
description: Drive the surrounding zz workspace (panes, windows, terminal output, agents, browser panes, other machines) with the `zz` CLI. Use whenever you are running inside zz (the `ZZ_PANE` environment variable is set) and the task involves another pane, a terminal's output, a browser page, or another machine.
metadata:
  zz-stub: "1"
---

Run `zz tools --skill` and follow the guide it prints. The guide comes from the installed zz, so it describes the commands this version has.
"#;

pub use zz_daemon_client::agent_context::{
    WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE, workspace_context,
};

const WORKSPACE_TOOLS_LOCAL_FLAGS: [&str; 1] = ["--context"];

macro_rules! workspace_tools_sections {
    ($($name:ident = $text:literal;)+) => {
        $(const $name: &str = $text;)+
        const WORKSPACE_TOOLS: &str = concat!($($text),+);
    };
}

workspace_tools_sections! {
    WORKSPACE_TOOLS_HEAD = r"# Workspace tools

Drive terminal and browser panes, and the agents in them, through the `zz` CLI. Run `zz tools`
for the full catalog or `zz tools agent` for agent verbs. Run `zz tools --skill`
for the agent guide with skill frontmatter. Regenerate it with `just tools skill`.
Run `zz tools terminal` for terminal verbs, `zz tools browser` for browser verbs,
and `zz tools advanced` for state subscriptions and terminal-agent integration.
This guide is long. If your tool shortened it, the middle is what went missing:
read that part on its own with one of those section commands.
`zz tools --context` prints the short startup context for the calling pane.
Run `zz tools --install-hooks` once so Claude Code and Codex started in a
terminal pane get it, and `zz tools --remove-hooks` to take the hook and stub
skill out again. Codex
skips a new or changed hook until you approve it with `/hooks` in Codex.
The install also adds each agent's own allow rule for `zz` (Claude Code's
`Bash(zz:*)`, a Codex `rules/zz.rules` file), so `zz` commands run without
asking, and Codex runs them outside its sandbox when the ids are written out
rather than held in variables.

## Environment

| Variable | Meaning |
| --- | --- |
| `ZZ_PANE` | Your own pane, e.g. `%3`. Do not send to yourself. |
| `ZZ_AGENT` | Set in a chat: the agent it runs (`claude`, `codex`, `agy`, `grok`, `cursor-agent`). |
| `ZZ_SESSION` | Your daemon session name. |
| `ZZ_SOCKET` | The daemon endpoint; the CLI honors it. |

Explicit values in `agent-command` config take precedence over these defaults.

## Targets

Start with a pane summary or selected fields:

```sh
zz inspect -t %3
zz list-panes -F '#{pane_id} #{pane_kind} #{agent_state} #{@agent_state}'
zz list-sessions
zz list-windows
```

Use `list-panes --json` for the full tmux variable set.
Use stable IDs: `%N` for a pane, `@N` for a window, `$N` for a session.
Pass the bare ID: `-t %3` works; `-t work:%3` fails with `can't find window`.
Run `zz <verb> --help` for options, or `zz list-commands [verb]` for usage.
`zz` without a verb attaches the terminal UI (`new-session -A`); with `--host NAME`, on that host.
Pane options need `-p`: `zz set-option -p -t %3 @name reviewer`.

`#{pane_kind}` is `terminal`, `browser`, `editor`, `picker`, `agent`, or `view`.
`#{pane_last_command_status}` is the last completed command's exit code, or empty
when unknown; terminal panes report it from OSC 133 marks.
`#{@name}` reads a user option from pane, window, session, then global scope.

## Placement

A pane goes where its work belongs. The user reads sessions, windows and panes as
a tree of what is going on, so a pane in the wrong place hides your work or
clutters someone else's.

- What serves your own task stays in your own window. A page or a preview goes
  beside you: `zz split-window -d -h -t %3 --kind browser URL`. A helper terminal
  (a server, a watcher, a log) is a small split,
  `zz split-window -d -v -l 6 -t %3 'COMMAND'`, or a background task of your own
  when nobody needs to watch it. Do not open a window for either.
- A chat you open for the user (a hand-off, a fork, work they watch) is a float
  in your window, which `new-pane --kind agent` gives you. Background work of
  your own is not a chat: see Agents.
- Work that is not part of the window you would put it in gets a session of its
  own, named for the task: `zz new-session -d -P -F '#{pane_id}' -s TASK -c DIR`
  prints its first pane. Never add a pane to a window that is open for something
  else, on this machine or another.
- A pane that landed in the wrong place moves with
  `zz join-pane -d -h -s %7 -t %3`, running program and all.

## Remote hosts

`zz list-hosts` lists the machines in `zz/config` with their state and version.
`zz --host NAME VERB` runs a verb there (NAME may be an ssh destination; a name that
`zz/config` does not list and that is this machine's hostname runs here), and
`zz --host all VERB` runs a read-only verb on every host, prefixing text lines with
`NAME: ` and adding `host_name` to JSON lines. Pane IDs are per host and none is
yours there: pass `-t %N` from `list-panes -a` or your own
`new-window -d -P -F '#{pane_id}'` (`new-session -d -P -F '#{pane_id}'` on a host
with no session); a verb that acts on a pane, window or session exits 2 without
`-t`, and a move or swap without `-s`. A pane from `list-panes -a` is a target for
acting on that pane, not a place to start new work: a task you start on a host
gets a session of its own. `zz --host NAME new-session -d -P -F '#{pane_id}' -s TASK -c DIR`
prints a terminal `%N`,
`zz --host NAME split-window -d -t %N -c DIR --kind agent claude 'BRIEF'` opens the
chat beside it, and `zz --host NAME kill-pane -t %N` closes the terminal, which
leaves the chat alone in its session. `zz --host NAME run-shell 'COMMAND'` runs a
command line there with no pane, prints its output and exits with its status.
Hints and errors that come back from a host name their commands with
`--host NAME` already: run them as printed. Paths and pane environments are that
host's; `events` and `agent-send --progress` stream from it. Logins use zz’s own
key after the first, and `--host all` never prompts. A call that changes state
and stalls 15 s exits 1 and may have run there: check before retrying. When a
machine joins, run `zz fleet add NAME DEST`, then `zz fleet sync`; run
`zz fleet install NAME` for a missing zz. `zz update NAME` installs a newer build
there beside its package, with no sudo, and `--restart -y` moves its daemon onto
it, which ends that host's sessions; `zz update --check -a` prints what every
machine runs, and `zz build-info` prints this build's version and wire floors.
A tailnet host gets `zz update-server` and `zz restart-server` from it, sent to
its daemon.

`zz copy-file [HOST:]SOURCE [HOST:]DESTINATION` copies one file to a host, from a host or
between two hosts, and prints the destination and its size. A path on a host is absolute or
starts with `~/`; a destination that is a folder here, or ends with `/` there, keeps the
file's name. It copies files only: pack a folder first, as in
`zz --host NAME run-shell 'tar czf /tmp/x.tgz -C ~/dir .'`, then `zz copy-file NAME:/tmp/x.tgz .`.
Each host needs `read-file`, so update an older one first.

## CLI contract

The `--help` flag alone, or the `help` verb, prints the command catalog. Use
`zz <verb> --help` for a command's description, usage, options, and positional
arguments; aliases and unique prefixes work too. These help forms need no daemon and exit 0. An
unknown verb exits 1; an unknown verb requested through zz’s `--help` exits 2.
Global `-h` keeps the tmux usage banner, and command `-h`
flags keep their tmux meaning.

Add `--json` to `list-sessions`, `list-windows`, `list-panes`, `list-clients`, or
`list-hosts` for one JSON object per row in the same order as text output. Keys are the format
variable names for that entity; values are strings with the same expansion as
`#{name}`, including empty strings for unavailable values. Pane rows include
`pane_kind`, `agent_state`, `agent_pending_permission`, `agent_parent`, `browser_url`,
`pane_pb_state`, `pane_pb_progress`, and the `pane_status*` program status
variables. Use `show-options --json` for one object
mapping option names to value strings in the selected scope. Combining `-F` and
`--json` is a usage error.

| Exit code | Meaning |
| --- | --- |
| 0 | Success. |
| 1 | Command failure, a stopped agent turn, missing daemon, connection loss, or a tmux-compatible usage error (including an unknown command). |
| 2 | Usage error in a zz-native verb or extension: invalid flag, missing argument, or malformed value. |
| 3 | Blocked or unable to answer now, including `agent-send --on-block fail`; for `run-pane`, nothing ran and a retry is safe. |
| 124 | Wait timed out, including `wait-pane --exit` and `agent-send`; a child may also return 124. |
| 125 | `run-pane` typed the command and it was still running at the timeout, or its child returned 125; the timeout writes a diagnostic to stderr. |

For `wait-pane`, `run-pane`, `send-text`, and `agent-send`,
`--timeout` takes seconds, accepts fractions such as 0.5, and 0 waits forever.

Tmux-compatible commands keep the pin’s exit status, including 1 for parse and usage errors.
The error’s source determines the status: `list-panes -Z` exits 1, while zz’s
`list-panes --json -F x` extension conflict exits 2.
Commands that set an explicit exit code keep that code.

";
    WORKSPACE_TOOLS_AGENT = r#"## Agents

Claude Code, Codex, Antigravity and Grok run two ways in a pane, and a pane flips between them on the same session. Cursor runs as a chat only (`agent cursor`): its CLI and its chats keep separate conversations, so it does not flip. In a terminal pane the CLI runs as its own TUI, started like any program (`zz split-window -h claude`). In an agent pane the daemon runs the CLI headless and the clients draw the conversation as a chat: `zz select-pane-kind -t %N [-c DIR] [--resume ID] [--model M] [--effort E] [--account ID] agent claude|codex|agy|grok|cursor` turns a picker or a terminal into one.

A chat is a pane in the user's sidebar, with its own transcript, model and approvals, and it stays there until the user closes it. Open one for work the user watches or steers: a task they asked you to hand to another agent, work they want to see or answer, a fork of a conversation, a task for another machine. Do not open chats as workers for your own task (searches, reading, edits you split up and collect): use your own subagents for that, or do the work yourself.

### `zz new-pane [-d] [-P -F FORMAT] [-c DIR] [-t TARGET] --kind agent claude|codex|agy|grok|cursor [--model M] [--effort E] [--account ID] [--resume ID] [BRIEF]`

Open a new chat as a float. `split-window` and `new-window` take the same `--kind agent` words and give the chat a split beside the target or a window of its own. `--model`, `--effort`, `--account` and `--resume` go before the provider or right after it. The words after them are the brief, sent as the chat's first prompt once its session is up, with the same checks and 1 MiB cap as `agent-send` TEXT; no brief opens an empty chat. Without `-c` the chat starts in the folder a terminal made the same way would. `-P -F '#{pane_id}'` prints the new pane, which takes `agent-send` at once: `zz new-pane -d -P -F '#{pane_id}' --kind agent codex 'review the diff in this folder'`. A missing or unknown provider, or a bad flag value, exits 2. Every create prints one line on stderr, `new-pane: opened %N (codex)` (with the verb's own name); with a brief it goes on `; its reply comes back to you as a message; end your turn now` when run from a chat, and `; read its reply with: zz agent-send -t %N --wait ''` when run from anywhere else.

Run from a chat, the new chat is yours: `#{agent_parent}` names your pane, its reply to the brief comes back to you as a message once its turn ends, and it approves its own tool calls unless `agent-auto-approve` is `off`, so only a question it asks comes to you, with the `agent-respond` lines that answer it. Do not wait or poll: while your turn is still running, Claude Code and Codex get these messages inside it, and the other agents get them as their next prompt. The chat you open gets a note with its brief, kept out of its transcript, saying which chat opened it and that its final message is its reply. It cannot open chats of its own (it is told to do the work itself or say what it needs), and closing your pane closes it and every pane it opened. If the user closes it and opens it again, `zz agent-send -t %N` still reaches it by the id you were given; while it stays closed, the send fails with `%N was closed; open a new chat with: zz new-pane -d --kind agent codex 'BRIEF'`. A message reads `[zz] %N (codex) finished:` or `stopped (REASON):`, then the reply and `If this answers the user, answer the user now and send nothing to %N. Only if something is missing: zz agent-send -t %N 'TEXT'` (inside a running turn that line says to carry on with what you were doing instead); `failed: MESSAGE` and `Tell the user, or try again with: zz agent-send -t %N 'TEXT'`; or `was closed before it replied. Tell the user; do not open it again.`; an ask reads `[zz] %N (codex) asks: KIND TITLE` and lists the `zz agent-respond` commands that answer it, one `--option ID REQUEST_ID  # LABEL` line per choice of a question and `--answer 'TEXT' REQUEST_ID` for one that wants typed text; each line ends with the id of the request it answers, so once that request is answered or gone it fails with `has no pending request` instead of answering another one.

A prompt the user wrote that names an agent with `@+claude`, `@+codex`, `@+agy`, `@+grok`, `@+cursor` or `@+codex/MODEL`, or a running chat with `@HOST:%N`, comes with a note, kept out of the transcript, that tells you to hand the task over with the exact command (`zz new-pane -d --kind agent codex 'BRIEF'`, or `zz agent-send -t %N 'TEXT'` for a chat) and end your turn. For a chat on another machine the command is `zz --host HOST agent-send -t %N 'TEXT'`. Run from a chat, its reply comes back to you as a message too: stderr says `agent-send: HOST:%N's reply comes back to you as a message; end your turn now`, and the message reads `[zz] HOST:%N (codex) finished:` with `zz --host HOST agent-send -t %N 'TEXT'` as its follow-up. That chat's permission asks stay with it and are answered there. When the reply cannot be routed (the command runs from a terminal, or this machine cannot log in to HOST without asking), stderr says `agent-send: read %N's reply with: zz --host HOST agent-send -t %N --wait ''`, which waits for the turn and prints the reply.

A chat answers the same reads as a terminal. `#{pane_current_command}` is `claude`, `codex`, `agy`, `grok` or `cursor-agent`. `#{agent_state}` is `starting`, `idle`, `working`, `needs-approval`, `needs-input` or `failed`, and `#{agent_pending_permission}` is the pending request ID or empty, in `list-panes -F`, `display-message -p` and `inspect`. `capture-pane -p -t %N` prints the chat's last turn as text: `> ` and the prompt, one line per tool call (its kind and title), then the reply; `-S -` prints the whole chat, and an empty chat prints nothing. While a turn runs, stderr says `capture-pane: %N's turn is still running`. `wait-pane` on a chat fails with `%N is an agent chat; wait for its turn with: zz agent-send -t %N --wait ''`, and `show-last-output` with `%N is an agent chat; read it with: zz capture-pane -p -t %N`. Typing into a chat fails and names the verb to use: `send-keys`, `send-text`, `paste-buffer` and `run-pane` fail with `%N is an agent chat; send with: zz agent-send -t %N TEXT`, and `pipe-pane` with `%N is an agent chat; read it with: zz capture-pane -p -t %N`.

### `zz flip-agent [-t %N] [--now | --when-idle | --cancel]`

Flip a terminal running Claude Code, Codex, Antigravity or Grok into a chat on the same session, or a chat into a terminal running the CLI with that session resumed, through the user's interactive shell. The pane keeps its id and place. When something runs (a turn, background shells or subagents, queued prompts) the flip fails with an error that starts with `busy:` and says what runs; `--now` flips anyway and stops it, `--when-idle` flips once nothing runs, and `--cancel` drops a waiting flip. Codex on its shared app server never refuses: the turn lives in the server and the other side picks it up.

### `zz agent-account [-t %N] [--now | --when-idle] ID|default`

Switch the machine's login of the chat's CLI (Claude Code or Codex) to another account and resume every chat of that CLI on it. The CLI keeps its one login in its own place and the other accounts' logins are held aside, so the CLI switches too when opened outside zz. IDs are the slots `zz agent-accounts` lists (`a1`, `a2`, ...); `default`, or the account already in use, changes nothing. While any chat of that CLI runs something, a Codex turn included, it fails with an error that starts with `busy:`; `--now` switches anyway and stops it, and `--when-idle` switches once none runs, unless another `agent-account` for that CLI comes first.

`zz select-pane-kind --resume ID --fork [--at MESSAGE] agent claude|codex` opens a chat on a copy of a session, through that message's turn when `--at` names one.

### `zz agent-send [-t %N] [--submit | --notify | --wait [--progress] [--timeout SECS] [--on-block wait|fail|allow|deny] [--json | --final]] [--context PATH[:START[-END]]] [TEXT]`

Send TEXT as a prompt to an agent pane (a chat), or to another agent in a terminal pane, typed into its input with Enter only when `--submit` is given. A terminal running Claude Code, Codex, Antigravity or Grok refuses it: flip the pane to chat first. Read stdin when TEXT is omitted: `git diff | zz agent-send -t %3`. `--context` adds a file/line header and fences the payload; text is capped at 1 MiB.

A chat gets the prompt under a first line that names who sent it: `[zz] %N (claude) sent this, not the user:` from a chat, `[zz] a command in %N sent this:` from a terminal pane and `[zz] a command sent this:` from anywhere else. A sender on another machine is written `HOST:%N`, or `a command on HOST` when it has no pane. The chat's clients draw it as a message from that sender, not as the user's prompt. A brief given to `new-pane` gets the same line; a send or a brief the app itself runs, from a key binding or its command prompt, is the user's own and gets none.

`--submit` sends now and prints the chosen pane. `--wait`, `--notify`, `--final`, `--json` and `--on-block` follow the chat's turn. `--notify` runs from inside another terminal pane and, when the target's turn ends, posts its reply back into the calling pane; from a chat it is a plain send, since the reply comes back anyway. `--wait` waits for the turn and prints its reply on stdout (pane ID on stderr). A terminal pane falls back to waiting on `@agent_state`, and `--notify`, `--final` and `--on-block allow|deny` exit 2 there. Failure, cancellation, hand-back, or timeout exits non-zero. The timeout defaults to 600 seconds; `0` waits forever. A timeout leaves the turn running. `--json` prints one object with turn facts, `final_text`, and `transcript`. `--final` prints only the text after the last tool call or tool update. Both require `--wait`; combining them is a usage error. `--on-block wait` waits for permission. `--on-block fail` prints the pending permission JSON and exits 3 while the turn continues. `--on-block allow` approves the pending permission and `--on-block deny` rejects it.

Turn facts: `final_text` contains message text after the last tool call or tool update. `tool_calls` counts the turn's tool calls; permission counters count the chat's requests. Each buffer keeps its tail when capped: 1 MiB for the transcript and 256 KiB for final text. The JSON object has `pane`, `stop_reason`, `duration_ms`, `tool_calls`, `permissions` (`requested`, `allowed`, `denied`), `truncated`, `final_text`, and `transcript`. A blocked reply also has a nested `permission` object. Exit 0 means `end_turn`; other stop reasons exit 1 with the reply still printed, except cancellation, which keeps its existing error output. A blocked wait exits 3 and a timeout exits 124. Stderr carries the pane ID and on-block audit lines. `--progress` requires `--wait` and an explicit `-t %N`. It prints tool calls, state changes, and a heartbeat after 60 seconds without a line to stderr. Titles in progress lines stop at 120 characters.

A send without `--wait` prints the pane ID. From a chat, the reply comes back to you as a message when that turn ends, the chat's permission asks come to you as they open, and stderr says `agent-send: %N's reply comes back to you as a message; end your turn now`; from anywhere else stderr says `agent-send: read %N's reply with: zz agent-send -t %N --wait ''`. A chat that already has 4 prompts waiting refuses a send with exit 1 and `%N has 4 prompts waiting; wait for its turn with: zz agent-send -t %N --wait ''`. With `--wait` from a chat, `--on-block` defaults to `fail`, and a blocked wait ends its output with the `agent-respond` and `agent-send --wait ''` lines to run next. A chat cannot send to the chat that opened it: its reply reaches that chat when its turn ends. A chat sends at most 8 messages to one chat (a brief counts as the first) between two prompts the user gives it; the next fails with `%N has had 8 messages from you since the user last wrote; answer the user now`. `--wait ''` on a chat waits for the turn that is running and prints its reply; with no turn running it exits 1 with `%N has no running turn; read its last reply with: zz capture-pane -p -t %N`.

### `zz agent-respond [-t %N] (--allow | --deny | --option ID | --answer TEXT) [REQUEST_ID]`

Answer the named or oldest pending permission in an agent pane. Read the pending request with `zz inspect -t %N --json | jq .permission`.
Print the chosen option ID. `--allow` approves once and `--deny` rejects. `--answer TEXT` types the answer to a request that asks one question and prints `answered request ID`. Other panes exit 1 with `is not an agent chat`, and a chat with nothing pending exits 1 with `%N has no pending request (already answered, or none was asked)`.

### Permissions

Terminal agents keep their own permission settings: Codex `approval_policy`, Claude Code permission rules. `zz tools --install-hooks` adds a rule that lets both run `zz` without asking.

### `zz inspect [-a | -t %N] [--json]`

Describe a pane's kind, process, geometry, exit status, progress, agent facts, and browser URL.
Read one `key: value` line per field, or use `--json` for one object with string facts and string arrays for `verbs` and `events`. Text output lists use spaces and skip empty facts; JSON keeps every key, with empty strings for unavailable facts. `agent_pending_permission` is the pending request ID, or empty. `agent_provider` (`claude`, `codex`, `agy`, `grok` or `cursor-agent`), `agent_model` (the model the chat was started with, empty for the default) and `agent_session_id` are set for agent panes, and `agent_parent` names the chat that opened this one. `verbs` lists what applies to this pane's kind; agent panes list `agent-send`, `capture-pane`, `flip-agent`, `agent-account` and `agent-respond`, a terminal running Claude Code, Codex, Antigravity or Grok lists `flip-agent`, and another terminal with a nonempty `agent_state` lists `agent-send`. `events` names what `zz events` can report for the pane. Use `-a` for every pane in every session, ordered by session ID, window index, and pane index, with a blank line between text blocks or one JSON object per line; `-a` and `-t` cannot be combined. With neither, inspect the current pane.

```sh
zz inspect -a --json | jq -c 'select(.agent_state!="") | {pane_id,agent_state}'
```

The keys, in text output order, are `session_id`, `session_name`, `window_id`, `window_index`, `window_name`, `window_width`, `window_height`, `window_size`, `pane_id`, `pane_index`, `pane_active`, `pane_kind`, `pane_pid`, `pane_current_command`, `pane_current_path`, `pane_title`, `pane_width`, `pane_height`, `pane_dead`, `pane_dead_status`, `pane_dead_signal`, `pane_last_command_status`, `pane_pb_state`, `pane_pb_progress`, `pane_status`, `pane_status_kind`, `pane_status_progress`, `pane_status_app`, `pane_status_title`, `pane_status_message`, `pane_status_raw_title`, `pane_status_raw_message`, `pane_status_reported`, `agent_state`, `agent_pending_permission`, `agent_provider`, `agent_model`, `agent_session_id`, `agent_parent`, `permission`, `browser_url`, `verbs`, `events`.

`permission` is a nested `{"request_id":7,"tool_call":{...},"options":[...]}` object or `null` in JSON output; text output prints compact JSON on the `permission:` line, or an empty value.

### `zz events [-t %N]`

Stream hook events.
Print JSON lines, flushed per line: `{"seq":1,"event":"agent-state-changed","time":1750000000000,"hook_pane":"%3","agent_state":"working",...}`. Each line includes the hook's string variables. `time` is Unix milliseconds. Wait for the first line, `{"seq":0,"event":"ready","time":...}`, before starting work. On subscriber overflow, `gap` consumes the next sequence number; the client reconnects and continues counting without another `ready` line. Use `-t %N` for a pane, `-t @N` for a window, `-t '$N'` for a session ID, or `-t name` for a session name. Filters match exact hook fields, and a session filter also passes the events of that session's windows and panes; `ready` and `gap` always print. Omit `-t` to stream without filtering. `agent-state-changed` fires when a terminal's `@agent_state` is written and when a chat's state or pending permission changes, and carries `agent_state` and `agent_pending_permission` (the permission ID or an empty string). Agent panes also emit `agent-tool-call` for new calls and completed or failed status changes. Each event carries `tool_call_id`, `tool_title`, `tool_kind`, and `tool_status`; titles collapse whitespace and stop at 200 characters: `{"seq":2,"event":"agent-tool-call","time":1750000000000,"hook_pane":"%3","tool_call_id":"call-1","tool_title":"cargo test","tool_kind":"execute","tool_status":"in_progress"}`. These agent events are stream-only; `set-hook` cannot bind them. Other `@option-changed` firings are not streamed; use a hook for those. Invalid arguments exit 2. Connection and daemon errors, or any disconnect other than overflow, exit 1, including server shutdown.

"#;
    WORKSPACE_TOOLS_TERMINAL = r#"## Terminal panes

Run `zz tools terminal` for terminal verbs.

### `zz split-window [-h | -v] [-t %N] [-d] [-P] [-F FORMAT] [-e KEY=VAL] [-c DIR] [COMMAND...]`

Split a terminal pane; `-t %N` chooses the pane to split. Execute COMMAND as argv without a shell.
Print nothing unless `-P` requests `session:window.pane`; add `-F '#{pane_id}'` for the pane ID. Use `-d` to keep focus, `-e` for environment values, and `-c` for the working directory.

Open a picker with `zz split-window --kind picker -v -P -F '#{pane_id}'`.

### `zz run-pane [-t %N] [--timeout SECS] [--] COMMAND...`

Run a command in a terminal pane's POSIX shell without shell integration.
One COMMAND word is a shell command line, run as written: use it for pipes, `&&`
and variables the pane's shell should expand. Several words are a program and its
arguments, each quoted so the pane's shell gets them as you wrote them. With no
pane to run in, `zz run-shell 'COMMAND'` runs a command line in a shell of the
daemon's, prints its output and exits with its status; with `--host NAME` that is
the way to run a command on another machine without making a pane there. It prints
stdout only, so end the command with `2>&1` to see its errors; it expands formats, so
write `##{` for a literal `#{`; and on Windows the shell is `cmd`. `run-pane` needs a
POSIX shell in the pane: in PowerShell, cmd, fish or nushell it refuses, and the way
there is `send-text`, `send-keys Enter`, then `wait-pane --until TEXT`.
Type it, verify the echo, then press Enter. Print the output between
unique markers and return the child's exit code. Capture includes scrollback,
capped to the last 10,000 logical lines; when the start scrolled past the pane's
`history-limit`, stderr says `run-pane: output truncated to its last N lines`, so
send big output to a file. One run-pane runs per pane; a second waits its turn.
Exit 3 means nothing ran, so a retry is safe: the pane stayed busy past the timeout,
its foreground program is not a POSIX shell (`run-pane: %N is running vim, not a shell`),
the shell was waiting for more input such as an unclosed quote (run-pane sent C-c),
the line is longer than that shell takes (put a long command in a script file),
or the echo never appeared (`nothing was submitted`).
The timeout covers waiting, echo and execution; it defaults to 120 seconds, accepts
fractions, and 0 waits forever. When the command was typed and still runs at the
timeout, print partial output to stdout, write `run-pane: timed out after <N>s on %N; the command keeps running`
to stderr, and exit 125. A child exit of 125 differs from a timeout only by that
stderr line. Use a command or Control client.

```sh
zz run-pane -t %3 -- "sh -c 'echo hi; exit 7'"
zz wait-pane -t %3 --until 'ready' --timeout 2.5
```

### `zz wait-pane [-t %N] [--idle MS | --until TEXT | --regex RE | --exit] [--timeout SECS] [--tail N]`

Choose one condition; the default is `--idle 500`, measured from this call.
Text and regex matches search logical lines on screen at the call or written since,
including scrollback, capped to the last 10,000 logical lines. Join wrapped lines
and print the matching line. Prompt and typed-command rows never match in panes with
OSC 133 marks; elsewhere anchor the pattern (`--regex '^DONE$'`) so the echo cannot. `--tail N` narrows the search to the last N logical
lines after skipping trailing blanks. Idle success prints nothing.
With `--exit`, wait for the PTY child and mirror its exit status, with no output.
A retained dead pane returns its status immediately; killing or respawning the
pane releases the wait with status 0. `--tail` with `--exit` is a usage error.
The timeout defaults to 60 seconds, accepts fractions, and 0 waits forever.
Timeout exits 124 and names the condition. Invalid regex syntax exits 2.
Use a command or Control client.

### `zz send-text -t %N [--no-enter] [--timeout SECS] [TEXT]`

Paste into a terminal TUI, wait for the text to appear, then press Enter. Read
stdin when TEXT is omitted. `--no-enter` drafts. The timeout defaults to 2 seconds,
accepts fractions such as 0.5, and 0 waits forever. Only the target pane gets the text,
even with `synchronize-panes` on.
Print nothing on success. If the text never appears, exit non-zero without submitting.
A paste that Claude Code or Codex folds into a placeholder (`[Pasted text #1 ...]`,
`[Pasted Content N chars]`) counts as appeared.

### `zz send-keys -t %N 'text' Enter`

Send raw keys to a terminal; `-l` sends literal text. Use `send-text` for composers
that swallow an Enter sent before the paste appears.

Print nothing on success.

### `zz capture-pane -p -t %N [-S -] [-E -] [-J]`

Print a terminal pane's text to stdout. `-S -`/`-E -` include the whole scrollback;
`-J` rejoins soft-wrapped lines.

### `zz show-last-output -t %N`

Print that last command and output under a `%N $ command` header, with the same
OSC 133 requirement and caps, soft-wrapped lines joined. When known, an `exit: <n>` line follows the header, or `exit: running`
while it runs; after a run-pane the header shows the command it ran.

### `zz send-last-output -t %N`

Type the last completed command and output into the input of the window's most
recently focused terminal running Claude Code or Codex, without pressing Enter.
The default binding is `<prefix> e`. Requires OSC 133 prompt marks;
the bundled Bash/zsh integration emits them, with best-effort PowerShell support.
Output is capped at 200 lines or 256 KiB with a truncation note.

Print `sent the last command from %N to %M` with the source and recipient pane IDs.

### `zz pipe-pane [-o] -t %N [COMMAND]`

Pipe pane output to COMMAND. `-o` starts a pipe only when none exists.
Print nothing; omit COMMAND to close the pipe.

```sh
zz pipe-pane -o -t %5 'cat >> /tmp/agent-output.log'
```

### `zz run-shell -b -d SECS COMMAND`

Schedule a background command after SECS seconds.
Return without printing or waiting; show command output in the most recent pane's command-output view.

```sh
zz run-shell -b -d 300 'zz send-text -t %5 continue'
```

"#;
    WORKSPACE_TOOLS_BROWSER = r"## Browser panes

Run `zz tools browser` for browser verbs.

### `zz split-window --kind browser [-h | -v] [-t %N] [--profile NAME] [-P] [-F FORMAT] [URL]`

Split a pane to open a browser; `-t %N` chooses the pane to split.
Use `zz new-window --kind browser [--profile NAME] [URL]` to open a browser window.
Both default to `about:blank`. Print nothing unless `-P` requests the tmux target;
add `-F '#{pane_id}'` to print the new pane ID.

A browser pane renders only while a desktop app window shows its session. With none,
`set-browser-url` and `html` just record the page and `capture-browser` fails; over
`--host`, open that host's session in the app first.

### `zz set-browser-url -t %N URL`

Navigate a browser pane. Print nothing on success. `#{browser_url}` reports the active tab's URL.
It returns before the page loads.

### `zz load-browser-url -t %N [--timeout SECS] URL`

Navigate a browser pane and return once the page has loaded, so a `capture-browser` after it
shows the new page. The window showing the pane reports the load: `#{@browser_loading}` is `1`
while a page loads and `#{@browser_loaded}` counts the loads that finished. A navigation inside
the page, which loads nothing, returns after 1.5 seconds. The timeout is 30 seconds by default
and `0` waits forever; a page still loading then exits 124 and keeps loading. A load counts
once the pane has stayed loaded for 0.2 seconds, so one that ends as the navigation starts is
not taken for the new page. With no window on
the pane's session, or an app that does not report loads, it sets the URL and fails.

### `zz html [--float] [--reply] [-t %N] PATH|-`

Show a page you wrote beside your pane: a file, a directory with `index.html`, or `-` to read
HTML from stdin. The pane reloads each time a file the page loaded changes, so rewrite the file
and look again. A file serves only itself; a directory also serves the files under it, except
names that start with `.`. Pages can load scripts from CDNs. `--float` opens a floating pane.
A pane keeps one page pane: your next `zz html` shows its page there instead of splitting again.
`-t %N` shows the page in that pane if `zz html` opened it, otherwise beside it. With
`--reply` the page can call `zz.reply(text)`, which sends the text to your pane. Print the page
URL and the new pane. Over `--host`, PATH
is read on that host; send a local page with `zz --host NAME html -t %N - < page.html`.

### `zz capture-browser -t %N -o /absolute/out.png`

Save the latest browser frame as a PNG and print its path. Use an absolute path on the daemon's
host: the window showing the pane sends the picture and the daemon writes it there, wherever
that window runs. A desktop app talking to an older daemon writes the file on its own machine
instead. A capture straight after `set-browser-url` can show the old page; navigate with
`load-browser-url` to wait for the new one. On Linux and Windows, restart the app with `ZZ_BROWSER_SHARED_TEXTURE=0` if the GPU
path reports that readback is unavailable.

### `zz capture-simulator -t %N -o /absolute/out.png`

Save an iOS simulator pane's screen as an upright PNG and print its path. The desktop app on
the Mac that shows the pane sends the picture and the daemon writes it on this host.

### `zz tap-simulator -t %N X Y`

Tap the simulator at a point of that PNG, in its pixels. `swipe-simulator -t %N X1 Y1 X2 Y2`
drags between two points, `type-simulator -t %N text ...` types with the simulator's keyboard
(ASCII only), and `press-simulator -t %N home|lock` presses a button. `--kind simulator UDID`
on `new-pane` opens a pane on a device; `xcrun simctl list devices booted` lists them.

### Browser through CDP

CDP is off by default. Set `browser-remote-debugging-port = 9222` in `zz/config`,
or `ZZ_BROWSER_REMOTE_DEBUGGING_PORT=9222` in the client environment (which takes
precedence). Use `0` to disable it; enabled ports range from 1024 to 65535.
Open a browser pane to initialize CEF. Restart the client to change the port after
initialization. The loopback endpoint grants local processes access to your browser
session; enable it while you intend to grant that access.

Replace PORT with the configured port in one of these attach recipes:

```sh
agent-browser --cdp PORT snapshot -i
npx @playwright/mcp@latest --cdp-endpoint http://127.0.0.1:PORT
npx chrome-devtools-mcp@latest --browserUrl http://127.0.0.1:PORT
```

Correlate pane URLs with CDP targets:

```sh
zz list-panes -a -F '#{pane_id} #{pane_kind} #{browser_url}'
zz display-message -p -t %5 '#{browser_url}'
curl http://127.0.0.1:PORT/json/list
```

`#{browser_url}` gives the active tab URL, or an empty string for other pane kinds.
Compare it with each target's `url`; duplicate URLs need more context, and CDP may
list background tabs too. For `Page.captureScreenshot` with a clip, use `clip.scale`
of `1.0`; CEF off-screen rendering does not support custom capture scale.

Match the pane to its CDP target before attaching. Never create pages (`Target.createTarget`,
Playwright `newPage`, or tools that open a fresh tab on connect): CEF hosts those
as native Chromium windows outside zz. Install agent-browser instead of invoking
`npx` per call. Prefer `wait --load load` or `wait --text` over `networkidle`.

```sh
agent-browser --cdp 9222 snapshot -i
agent-browser --cdp 9222 click @e2
agent-browser --cdp 9222 wait --load load
```

";
    WORKSPACE_TOOLS_ADVANCED = r##"## Advanced state

Run `zz tools advanced` for state subscriptions and terminal-agent integration.

### `zz read-file [-s OFFSET] [-n BYTES] PATH` and `zz write-file [-a] PATH HEX`

The two steps `copy-file` is made of, on the daemon's host. `read-file` prints up to 262144
bytes of a file from OFFSET as lowercase hex; fewer bytes than asked means the file ended.
`write-file` replaces the file with the bytes HEX spells, or appends them with `-a`. PATH is
absolute or starts with `~/`.

### State and waiting

Read agent state through formats.
A terminal pane whose program reports OSC 7501 program status gets its most urgent
record mapped into `@agent_state`: `working`, `blocked`, `failed` for `error`, and
`idle` for `idle`, `done`, or no record. Once a pane has reported, the two heuristics
below stop writing `@agent_state` for it until a full reset.
Terminal panes running a listed agent CLI get `working`/`idle` from the OSC 9;4
progress bar through `#{agent_state}` and `@agent_state`. `@agent-progress-commands`
sets the whitespace- or comma-separated list of command basenames (default `claude`).
A terminal pane running Claude Code gets `working`/`idle` from Claude Code's own
session status in its peer registry once per second, and `blocked` while Claude Code
waits on a permission prompt or a question. Set `@agent-peer-state off`
at pane, window, session, or global scope to disable these updates.

```sh
zz list-panes -F '#{pane_id} #{agent_state} #{agent_pending_permission}'
zz agent-send -t %5 --wait "..."
```

A completed turn or permission request also rings
the pane bell; use `zz set-hook -g alert-bell 'display-message "agent needs attention"'`.

For foreign agents, read `@agent_state` with `zz show-options -p -t %5 -v @agent_state`.
Use `zz agent-send -t %5 --wait "..."` to send and wait for a turn, and use the
lifecycle hooks below to write the state. For transitions you did not cause,
`zz wait-for '@agent_state@%5'` observes a terminal's option writes; read the state
after waking.
`zz set-hook -g @option-changed` observes user-option writes.

The sticky channel is level-triggered like tmux's `wait-for`: a signal that happened
before the wait wakes it at once. A read-then-wait loop started right after a send
can see the pre-send idle and return early. Use `zz agent-send -t %N --wait "..."`
to wait for a turn on any pane kind. Use the channel loop to observe transitions
you did not cause.
`zz events -t %N` streams `agent-state-changed` lines for any pane kind.
`zz wait-for -E -F '#{==:#{pane},%N}' agent-state-changed` blocks until that pane's
next state change; add `-v` to print its `agent_state`.

The `zz refresh-client` verb supports `-B` subscriptions in control mode. Send this
on the control connection and read `%subscription-changed`:

```text
refresh-client -B 'agent:%5:#{agent_state}'
```

### Sending to terminal agents

A terminal pane running Claude Code or Codex takes no prompts from `agent-send`:
flip it to a chat with `zz flip-agent -t %N` and send there.

For a terminal pane running another agent, `--wait` requires a non-empty `@agent_state` and types the text. It waits for a
non-idle state followed by `idle`, and prints nothing on success in text mode.
`--json` prints turn facts with empty reply text. There, `--final`, `--notify` and
`--on-block allow|deny` exit 2 before sending. A pane that
starts idle must leave idle within 15 seconds; otherwise it exits 124. The overall
`--timeout` also exits 124. `failed` exits 1; `blocked` waits unless `--on-block fail`
requests exit 3. A missing `@agent_state` exits 1 before sending.

Terminal waits report their known duration and reply text with zero counters.

The inbox wait also ends when the pane's turn ends (`#{agent_state}` leaves `idle` and
comes back), printing the SendMessage reply if one came and nothing if the session
answered only in its pane; `blocked` exits 3 under `--on-block fail` there too.

`zz agent-send -t %N --wait ''` sends nothing, on any terminal agent: it returns at
once when the pane is idle and otherwise waits for the running turn to end, printing
nothing (read the pane), or a SendMessage reply that reached the inbox after an
earlier inbox wait timed out or its caller went away; the daemon keeps those for an
hour. It takes `--timeout`, `--json` and `--on-block wait|fail`. Use it to wait again
after a timeout, or after a restart, without nudging the agent.

### Peers

Use `zz agent-send -t %N --wait "..."` to send and wait for a turn. Peers are per
machine: for an agent on another host, use `zz --host NAME agent-send -t %N --wait "..."`.

A terminal pane becomes a peer when you set its pane `@name` option:
`zz set-option -p -t %3 @name codex-1`. A Codex pane with a session name receives
peer messages through the Codex queue described above. Other terminal peers,
and Codex sessions without a name yet, receive pasted and submitted text, so do
not name a bare shell pane. A queue lookup or delivery failure leaves the message
unsubmitted and logs the reason. The daemon hosts no vendor's server. Claude Code
terminal sessions keep their own registration.

### Foreign agents in terminal panes

A terminal agent other than Claude Code or Codex has no chat. Configure its lifecycle hooks (Stop,
Notification, or equivalent) to write a pane option:

```sh
zz set-option -p -t "$TMUX_PANE" @agent_state idle
zz set-option -p -t "$TMUX_PANE" @agent_state needs-approval
zz agent-send -t %5 --wait "..."
zz set-hook -g @option-changed 'run-shell "notify-send zz \"#{hook_target} #{hook_option}\""'
```

Each write signals the sticky channel `<option>@<pane>` and runs `@option-changed`.
Use `#{hook_target}` and `#{hook_option}` to identify the write.

"##;
    WORKSPACE_TOOLS_ETIQUETTE = r"## Etiquette

- Send to another person's agent only for an authorized hand-off.
- Do not send to `$ZZ_PANE`.
- Trim piped logs to the part the recipient needs.

### `zz debug-marker [NOTE]`

Write a `user_marker` line to the daemon log for later diagnostics.
Print nothing on success.
";
}

fn gui_client(inner: &ServerState, session: SessionId) -> Option<(ClientId, Arc<OutboundMailbox>)> {
    let clients = inner.attachment.attached.get(&session)?;
    let find = |desktop: bool| {
        clients.iter().find_map(|id| {
            let client = inner.client(*id)?;
            let subscriber = client.subscriber.as_ref()?;
            (client.kind == Some(ClientKind::Interactive) && (client.path_picker || !desktop))
                .then(|| (*id, Arc::clone(subscriber)))
        })
    };
    find(true).or_else(|| find(false))
}

fn ensure_browser_attached(inner: &ServerState, pane: PaneId) -> Result<(), ServerError> {
    let window = inner
        .engine
        .state
        .window_for_pane(pane)
        .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
    let session = inner.engine.state.windows[&window].session;
    gui_client(inner, session)
        .map(|_| ())
        .ok_or(ServerError::PaneNotAttached(pane))
}

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

#[cfg(windows)]
enum QueuedCommand {
    Request(CommandRequest),
    Exec(zz_protocol::ExecRequest),
}

#[cfg(all(unix, test))]
fn handle_connection<S: TransportStream>(
    stream: S,
    shared: &Arc<Shared>,
) -> Result<(), DaemonError> {
    event_loop::serve_single(stream, shared)
}

#[cfg(windows)]
fn handle_connection<S: TransportStream>(
    mut stream: S,
    shared: &Arc<Shared>,
) -> Result<(), DaemonError> {
    let mut inbound_frame = Vec::new();
    let first_message = match read_protocol_message_into(&mut stream, &mut inbound_frame) {
        Ok(message) => message,
        Err(error @ ProtocolError::VersionMismatch { received, .. }) => {
            best_effort_protocol_mismatch_reply(&mut stream, received);
            log::info!(
                target: "zz_daemon::diagnostics::connection",
                "rejected client protocol={received} server_protocol={PROTOCOL_VERSION}",
            );
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    };
    handle_connection_message(stream, shared, first_message)
}

#[cfg(windows)]
fn handle_connection_message<S: TransportStream>(
    mut stream: S,
    shared: &Arc<Shared>,
    first_message: ProtocolMessage,
) -> Result<(), DaemonError> {
    let connection_started = diagnostic_timer();
    let mut inbound_frame = Vec::new();
    let (hello, compact_hello) = match first_message {
        ProtocolMessage::Hello(hello) => (hello.clone().into_client(), Some(hello)),
        ProtocolMessage::ClientHello(hello) => (hello, None),
        ProtocolMessage::Exec(request) => return exec::serve_exec(stream, shared, request),
        _ => {
            return Err(ServerError::InvalidCommand(
                "first protocol message must be ClientHello".to_owned(),
            )
            .into());
        }
    };
    if let Err(error) = validate_hello(&hello) {
        best_effort_error_reply(&mut stream, error.clone());
        log::info!(
            target: "zz_daemon::diagnostics::connection",
            "rejected client hello protocol={} revision={} server_protocol={PROTOCOL_VERSION}",
            hello.protocol_version,
            hello.revision,
        );
        return Err(error.into());
    }
    let startup_reentry_capability = format!(
        "{}{}",
        zz_daemon_client::STARTUP_REENTRY_CAPABILITY_PREFIX,
        shared.server_id
    );
    let startup_reentry = hello.kind == ClientKind::Command
        && hello.capabilities.contains(&startup_reentry_capability);
    if !startup_reentry && !shared.wait_for_startup() {
        return Ok(());
    }

    let outbound = OutboundMailbox::new();
    outbound.accept_client_capabilities(&hello.capabilities);
    let registration = if let Some(hello) = &compact_hello {
        shared
            .register_welcome(hello)
            .map(|(client, welcome)| (client, ProtocolMessage::Welcome(welcome)))
    } else {
        shared
            .register(
                hello.kind,
                hello.client_instance_id,
                hello.device_name.clone(),
                hello.color_scheme,
                hello.kind == ClientKind::Interactive
                    && hello
                        .capabilities
                        .iter()
                        .any(|capability| capability == ClientHello::CLIENT_TERMINAL_CAPABILITY),
                startup_reentry,
            )
            .map(|(client, hello)| (client, ProtocolMessage::ServerHello(Box::new(hello))))
    };
    let Some((client, greeting)) = registration else {
        best_effort_server_stopping_reply(&mut stream);
        return Ok(());
    };
    {
        let mut inner = shared.inner.lock();
        if hello.kind == ClientKind::Interactive
            && hello.capabilities.iter().any(|capability| {
                capability == ClientHello::CLIENT_NATIVE_TERMINAL_SEARCH_CAPABILITY
            })
        {
            inner.client_entry(client).native_terminal_search = true;
        }
        if hello.kind == ClientKind::Interactive
            && hello
                .capabilities
                .iter()
                .any(|capability| capability == ClientHello::CLIENT_NATIVE_CHOOSER_CAPABILITY)
        {
            inner.client_entry(client).native_chooser = true;
        }
        if hello.kind == ClientKind::Interactive
            && hello
                .capabilities
                .iter()
                .any(|capability| capability == ClientHello::CLIENT_PATH_PICKER_CAPABILITY)
        {
            inner.client_entry(client).path_picker = true;
        }
        let registered = inner.client_mut(client).expect("registered client");
        registered.origin = hello.origin;
        registered.nested = client_nested_fact(&hello.capabilities);
        registered.remote = client_remote_fact(&hello.capabilities);
        registered.utf8 = client_utf8_fact(&hello.capabilities);
        registered.exits_on_detach = attach::client_exits_on_detach_fact(&hello.capabilities);
        let features = client_features_fact(&hello.capabilities);
        registered.features = (features != 0).then_some(features);
        registered.tty = client_tty_fact(&hello.capabilities);
        registered.size = client_size_fact(&hello.capabilities);
        registered.cell_pixels = attach::client_cell_fact(&hello.capabilities);
        if let Some(viewport) = compact_hello.as_ref().and_then(|hello| hello.viewport) {
            registered.size = Some((viewport.columns, viewport.rows));
            registered.cell_pixels = Some((viewport.cell_width_px, viewport.cell_height_px));
        }
        registered.pid = Some(hello.process_id);
        registered.working_directory =
            client_working_directory_fact(hello.working_directory.as_ref());
        registered.environment = Some(client_environment_fact(&hello.environment));
    }
    shared.warm_terminfo(&hello.environment)?;
    let mut registration = ClientRegistrationGuard::new(shared, client);
    shared.client_lifecycle_hook("client-created", client);
    log::debug!(
        target: "zz_daemon::diagnostics::connection",
        "registered client={client} kind={:?} hello={hello:#?}",
        hello.kind,
    );
    let mut writer = stream.try_clone()?;
    let _ = writer.set_send_buffer_size(attach::MAX_BATCHED_WRITE_BYTES);
    #[cfg(unix)]
    if compact_hello.is_some() && hello.kind == ClientKind::Control {
        let socket = stream.receive_fd().ok();
        #[cfg(target_vendor = "apple")]
        let socket = socket
            .filter(|socket| rustix::net::sockopt::set_socket_nosigpipe(socket, true).is_ok());
        outbound.state.lock().quiet_socket = socket;
    }
    let writer_mailbox = Arc::clone(&outbound);
    let writer_shared = Arc::downgrade(shared);
    let writer_thread = attach::spawn_writer(&shared.connection_threads, move || {
        write_outbound(&mut writer, &writer_mailbox, &writer_shared, client);
    })
    .map_err(|error| DaemonError::Thread(error.to_string()))?;
    let mut writer_registration =
        ClientWriterRegistrationGuard::new(shared, client, Arc::clone(&outbound));
    if compact_hello.is_some() {
        let mut state = outbound.state.lock();
        state.attach_batch = true;
        state.ctrl_collecting = ControlCollection::Attach;
        state.terminals_held = true;
    }
    let _ = outbound.enqueue_reliable(&greeting);
    if matches!(hello.kind, ClientKind::Interactive | ClientKind::Control) {
        shared.subscribe(client, Arc::clone(&outbound));
    }
    if hello.kind == ClientKind::Control
        && hello
            .capabilities
            .iter()
            .any(|capability| capability == ClientHello::STARTUP_CONFIG_OWNER_CAPABILITY)
    {
        shared.try_deliver_startup_config_causes(client, &outbound, true);
    }

    let mut context = {
        let inner = shared.inner.lock();
        hello
            .origin
            .and_then(|pane| ExecutionContext::for_pane(&inner.engine.state, pane))
            .or_else(|| {
                let fallback = if matches!(hello.kind, ClientKind::Command | ClientKind::Control) {
                    inner.engine.state.most_recent_context()
                } else {
                    inner.engine.state.default_context()
                };
                fallback.map(|(session, window, pane)| {
                    ExecutionContext::new(Some(session), Some(window), Some(pane))
                })
            })
            .unwrap_or_default()
    };
    if let Some(hello) = &compact_hello {
        shared.initialize_compact(client, hello, &outbound, &mut context);
    }
    let command_queue_cancel = Arc::new(AtomicBool::new(false));
    let (command_sender, command_worker, mut context) =
        if matches!(hello.kind, ClientKind::Command | ClientKind::Control) {
            let (sender, receiver) = crossbeam_channel::unbounded::<QueuedCommand>();
            let worker_shared = Arc::clone(shared);
            let worker_outbound = Arc::clone(&outbound);
            let worker_cancel = Arc::clone(&command_queue_cancel);
            let worker_kind = hello.kind;
            let reader_context = (worker_kind == ClientKind::Control).then(|| context.clone());
            shared
                .command_queue_cancels
                .lock()
                .insert(client, Arc::clone(&command_queue_cancel));
            let worker = match thread::Builder::new()
                .name(format!("zz-client-command-{}", client.0))
                .spawn(move || {
                    let mut context = context;
                    let mut select = crossbeam_channel::Select::new();
                    select.recv(&receiver);
                    while let Ok(work) = select.select().recv(&receiver) {
                        if worker_cancel.load(Ordering::Acquire) {
                            continue;
                        }
                        let QueuedCommand::Request(CommandRequest {
                            request_id,
                            command,
                            prepared,
                        }) = work
                        else {
                            if let QueuedCommand::Exec(request) = work {
                                worker_shared.execute_compact_request(
                                    client,
                                    worker_kind,
                                    &mut context,
                                    request,
                                    &worker_outbound,
                                );
                            }
                            continue;
                        };
                        if worker_kind == ClientKind::Control {
                            sync_context_with_attachment(
                                &worker_shared.inner.lock(),
                                client,
                                &mut context,
                            );
                        }
                        let item = worker_shared.command_item(
                            (worker_kind == ClientKind::Control).then_some((client, request_id)),
                        );
                        let _ = item.execute_command_request_with_prepared_into(
                            &worker_outbound,
                            client,
                            worker_kind,
                            &mut context,
                            request_id,
                            &command,
                            prepared,
                        );
                    }
                }) {
                Ok(worker) => worker,
                Err(error) => {
                    shared.command_queue_cancels.lock().remove(&client);
                    outbound.close();
                    let _ = writer_thread.join();
                    return Err(DaemonError::Thread(error.to_string()));
                }
            };
            (Some(sender), Some(worker), reader_context)
        } else {
            (None, None, Some(context))
        };

    let mut path_list: Option<(u64, Arc<AtomicBool>)> = None;
    let path_list_turn = Arc::new(Mutex::new(()));
    let terminal_themes = Arc::new(Mutex::new(None));
    let mut stream = attach::inbound_reader(stream);
    let result = loop {
        let message = match read_protocol_message_into(&mut stream, &mut inbound_frame) {
            Ok(message) => message,
            Err(ProtocolError::Io(error))
                if matches!(
                    error.kind(),
                    ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::BrokenPipe
                ) =>
            {
                break Ok(());
            }
            Err(error) => break Err(error.into()),
        };
        let message_started = diagnostic_timer();
        log::trace!(
            target: "zz_daemon::diagnostics::connection",
            "message begin client={client} bytes={} frame_capacity={} message={:#?}",
            inbound_frame.len(),
            inbound_frame.capacity(),
            zz_daemon_client::TracedMessage(&message),
        );
        if shared.shutdown_pending.load(Ordering::Acquire)
            && !(hello.kind == ClientKind::Command
                && matches!(
                    message,
                    ProtocolMessage::CommandRequest(_)
                        | ProtocolMessage::PrepareCommandList { .. }
                        | ProtocolMessage::ClientFileResponse(_)
                ))
        {
            continue;
        }
        match message {
            ProtocolMessage::Exec(request) => {
                if let Some(sender) = &command_sender {
                    if sender.send(QueuedCommand::Exec(request)).is_err() {
                        break Ok(());
                    }
                } else if let Some(context) = context.as_mut() {
                    shared.execute_compact_request(client, hello.kind, context, request, &outbound);
                }
            }
            ProtocolMessage::TreeSync => {
                shared.send_compact_state(client, &outbound, true);
            }
            ProtocolMessage::ControlStdio => {
                let _ = outbound.enqueue_reliable(&ProtocolMessage::ControlStdioClosed);
            }
            ProtocolMessage::TtyInput { handoff }
            | ProtocolMessage::TtyInputRelease { handoff } => {
                let _ = outbound.enqueue_reliable(&ProtocolMessage::TtyInputClosed { handoff });
            }
            ProtocolMessage::GetKeyTables => {
                let tables = shared.inner.lock().engine.keys.snapshot();
                Shared::send_event(&outbound, EventPayload::KeyTablesChanged { tables });
            }
            ProtocolMessage::CommandRequest(CommandRequest {
                request_id,
                command,
                prepared,
            }) => {
                shared
                    .inner
                    .lock()
                    .attachment
                    .cold_bootstrap
                    .command(client);
                if command_sender.is_none()
                    && let Some(context) = context.as_mut()
                {
                    sync_context_with_attachment(&shared.inner.lock(), client, context);
                }
                if let Some(sender) = &command_sender {
                    if sender
                        .send(QueuedCommand::Request(CommandRequest {
                            request_id,
                            command,
                            prepared,
                        }))
                        .is_err()
                    {
                        break Err(DaemonError::Thread(
                            "command client worker stopped".to_owned(),
                        ));
                    }
                } else {
                    let _ = shared.execute_command_request_with_prepared_into(
                        &outbound,
                        client,
                        hello.kind,
                        context.as_mut().expect("interactive client owns context"),
                        request_id,
                        &command,
                        prepared,
                    );
                }
            }
            ProtocolMessage::ClientFileResponse(response) => {
                shared.complete_client_file(client, response);
            }
            ProtocolMessage::IdentityChallenge { nonce } => {
                if let Some(proof) = crate::tailnet::identity_proof(&nonce) {
                    let _ = outbound.enqueue_reliable(&proof);
                }
            }
            ProtocolMessage::HomeDirectoryRequest { request_id, users } => {
                let homes = shared.resolve_home_directories(&users);
                let _ = outbound.enqueue_reliable(&ProtocolMessage::HomeDirectoryResponse {
                    request_id,
                    homes,
                });
            }
            ProtocolMessage::TerminalThemesRequest { request_id } => {
                send_terminal_themes(&outbound, &terminal_themes, request_id);
            }
            ProtocolMessage::FileReadRequest {
                request_id,
                pane,
                path,
            } => {
                shared.start_file_read(client, hello.kind, (request_id, pane, path), &outbound);
            }
            ProtocolMessage::FileWriteRequest {
                request_id,
                pane,
                path,
                offset,
                last,
                bytes,
            } => {
                shared.receive_file_write(
                    client,
                    hello.kind,
                    &file_edit::FileWriteChunk {
                        request_id,
                        pane,
                        path,
                        offset,
                        last,
                        bytes,
                    },
                    &outbound,
                );
            }
            ProtocolMessage::AgentAttachFile {
                request_id,
                pane,
                name,
                bytes,
            } => {
                shared.save_attachment(
                    client,
                    hello.kind,
                    (request_id, pane, name, bytes),
                    &outbound,
                );
            }
            ProtocolMessage::PathListRequest {
                request_id,
                pane,
                dir,
            } => {
                if let Some((_, cancel)) = path_list.take() {
                    cancel.store(true, Ordering::Release);
                }
                let cancel = Arc::new(AtomicBool::new(false));
                path_list = Some((request_id, Arc::clone(&cancel)));
                shared.start_path_list(
                    client,
                    hello.kind,
                    (request_id, pane, dir),
                    &outbound,
                    (&cancel, &path_list_turn),
                );
            }
            ProtocolMessage::PathListCancel { request_id } => {
                if path_list
                    .as_ref()
                    .is_some_and(|(active, _)| *active == request_id)
                    && let Some((_, cancel)) = path_list.take()
                {
                    cancel.store(true, Ordering::Release);
                }
            }
            ProtocolMessage::EnvironmentRequest { request_id, names } => {
                let values = shared.resolve_environment(&names);
                let _ = outbound
                    .enqueue_reliable(&ProtocolMessage::EnvironmentResponse { request_id, values });
            }
            ProtocolMessage::PrepareCommandList {
                request_id,
                commands,
            } => {
                let commands = shared.prepare_command_list_for_request(client, commands);
                let _ = outbound.enqueue_reliable(&ProtocolMessage::PreparedCommandList {
                    request_id,
                    commands,
                });
            }
            ProtocolMessage::Attach { session } => {
                let Some(context) = context.as_mut() else {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand(
                                "command client cannot attach".to_owned(),
                            ),
                            output: RawText::default(),
                        },
                    ));
                    continue;
                };
                outbound.hold_terminals();
                match shared.attach_target(client, hello.kind, context, &session) {
                    Ok((session, snapshot)) => {
                        shared.clear_pending_committed_text(client);
                        shared.send_attached(client, &outbound, session, snapshot);
                        shared.publish_snapshot();
                        outbound.release_terminals();
                    }
                    Err(error) => {
                        outbound.release_terminals();
                        let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                            CommandResponse::Error {
                                request_id: 0,
                                error,
                                output: RawText::default(),
                            },
                        ));
                    }
                }
            }
            ProtocolMessage::Detach => {
                let control_session = (hello.kind == ClientKind::Control)
                    .then(|| {
                        shared.inner.lock().attachment.attached.iter().find_map(
                            |(session, clients)| clients.contains(&client).then_some(*session),
                        )
                    })
                    .flatten();
                shared.detach(client);
                if let Some(session) = control_session {
                    shared
                        .publish_to_client(client, EventPayload::detached_requested(session, None));
                }
            }
            ProtocolMessage::SetColorScheme(color_scheme) => {
                if hello.kind == ClientKind::Interactive {
                    shared.set_client_color_scheme(client, color_scheme);
                }
            }
            ProtocolMessage::SetConfigOverrides { entries } => {
                shared.set_config_overrides(client, hello.kind, &entries);
            }
            ProtocolMessage::SetTerminalPreview { enabled } => {
                shared.set_terminal_preview(client, hello.kind, enabled);
            }
            ProtocolMessage::ClientTerminalFeatures { features } => {
                shared.add_client_terminal_features(client, hello.kind, &features);
            }
            ProtocolMessage::ClientTerminalType { term_type } => {
                shared.set_client_terminal_type(client, hello.kind, &term_type);
            }
            ProtocolMessage::Input(input) => {
                if let Some(context) = context.as_mut() {
                    sync_context_with_attachment(&shared.inner.lock(), client, context);
                }
                if hello.kind != ClientKind::Interactive {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand(
                                "command client cannot send input".to_owned(),
                            ),
                            output: RawText::default(),
                        },
                    ));
                    continue;
                }
                let Some(context) = context.as_mut() else {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand(
                                "command client cannot send input".to_owned(),
                            ),
                            output: RawText::default(),
                        },
                    ));
                    continue;
                };
                if let Err(error) = shared.input(client, hello.kind, context, input) {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: daemon_server_error(error),
                            output: RawText::default(),
                        },
                    ));
                }
            }
            ProtocolMessage::GuiResponse(response) => {
                shared.complete_gui_request(client, response);
            }
            ProtocolMessage::BrowserScreenshot {
                request_id,
                last,
                png,
            } => {
                shared.receive_browser_screenshot(client, request_id, last, &png);
            }
            ProtocolMessage::Resync => shared.send_resync(client, &outbound),
            ProtocolMessage::RequestFull { pane } => {
                shared.request_full(client, pane, &outbound);
            }
            ProtocolMessage::HistoryRequest { pane, start, count } => {
                shared.send_history(client, pane, start, count, &outbound);
            }
            ProtocolMessage::PasteUploadBegin {
                upload_id,
                pane,
                purpose,
                extension,
                total_bytes,
            } => {
                shared.begin_paste_upload(
                    client,
                    hello.kind,
                    upload_id,
                    pane,
                    purpose,
                    extension,
                    total_bytes,
                );
            }
            ProtocolMessage::PasteUploadChunk { upload_id, bytes } => {
                shared.extend_paste_upload(client, upload_id, &bytes);
            }
            ProtocolMessage::FetchPastedImage { pane, number }
                if hello.kind == ClientKind::Interactive =>
            {
                shared.fetch_pasted_image(client, pane, number);
            }
            ProtocolMessage::FetchArtifact {
                request_id,
                token,
                path,
            } if hello.kind == ClientKind::Interactive => {
                shared.fetch_artifact(client, request_id, &token, &path);
            }
            ProtocolMessage::ArtifactReply { token, text }
                if hello.kind == ClientKind::Interactive =>
            {
                shared.reply_from_artifact(client, &token, &text);
            }
            ProtocolMessage::AgentLogin(request) if hello.kind == ClientKind::Interactive => {
                if shared.inner.lock().attachment.client_flags.contains(client) {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand("client is read-only".to_owned()),
                            output: RawText::default(),
                        },
                    ));
                } else {
                    shared.handle_agent_login(request);
                }
            }
            message @ (ProtocolMessage::AgentPrompt { .. }
            | ProtocolMessage::AgentCancel { .. }
            | ProtocolMessage::AgentUnqueue { .. }
            | ProtocolMessage::AgentRespondPermission { .. }
            | ProtocolMessage::AgentSetConfigOption { .. }
            | ProtocolMessage::AgentSetMode { .. }
            | ProtocolMessage::AgentAuthenticate { .. }
            | ProtocolMessage::AgentSessionOp { .. }
            | ProtocolMessage::AgentReplay { .. }
            | ProtocolMessage::AgentUnwatch { .. }
            | ProtocolMessage::AgentAcknowledgePromptRestore { .. }
            | ProtocolMessage::AgentAnswerQuestion { .. }
            | ProtocolMessage::AgentStopTask { .. }
            | ProtocolMessage::AgentSteer { .. }) => {
                let read_only_blocked =
                    !matches!(
                        message,
                        ProtocolMessage::AgentReplay { .. }
                            | ProtocolMessage::AgentUnwatch { .. }
                            | ProtocolMessage::AgentAcknowledgePromptRestore { .. }
                    ) && shared.inner.lock().attachment.client_flags.contains(client);
                if read_only_blocked {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error: ServerError::InvalidCommand("client is read-only".to_owned()),
                            output: RawText::default(),
                        },
                    ));
                } else if let Err(error) = handle_agent_message(shared, client, message) {
                    let _ = outbound.enqueue_reliable(&ProtocolMessage::CommandResponse(
                        CommandResponse::Error {
                            request_id: 0,
                            error,
                            output: RawText::default(),
                        },
                    ));
                }
            }
            _ => {}
        }
        log::trace!(
            target: "zz_daemon::diagnostics::connection",
            "message end client={client} elapsed_us={} context={context:#?}",
            diagnostic_elapsed_us(message_started),
        );
    };

    command_queue_cancel.store(true, Ordering::Release);
    if let Some((_, cancel)) = path_list.take() {
        cancel.store(true, Ordering::Release);
    }
    shared.detach(client);
    registration.unregister();
    drop(command_sender);
    if command_worker.is_some_and(|worker| worker.join().is_err()) {
        log::error!(
            target: "zz_daemon::diagnostics::connection",
            "command worker panicked for client={client}",
        );
    }
    shared.command_queue_cancels.lock().remove(&client);
    outbound.close();
    if writer_thread.join().is_err() {
        log::error!(
            target: "zz_daemon::diagnostics::connection",
            "writer thread panicked for client={client}",
        );
    }
    writer_registration.unregister();
    log::debug!(
        target: "zz_daemon::diagnostics::connection",
        "unregistered client={client} success={} connection_elapsed_us={}",
        result.is_ok(),
        diagnostic_elapsed_us(connection_started),
    );
    result
}

#[cfg(feature = "agent")]
fn handle_agent_message(
    shared: &Arc<Shared>,
    client: ClientId,
    message: ProtocolMessage,
) -> Result<(), ServerError> {
    shared.agent_message(client, message)
}

#[cfg(not(feature = "agent"))]
fn handle_agent_message(
    _shared: &Arc<Shared>,
    _client: ClientId,
    _message: ProtocolMessage,
) -> Result<(), ServerError> {
    Err(ServerError::UnsupportedCommand(
        "this daemon was built without agent support".to_owned(),
    ))
}

#[cfg(windows)]
fn best_effort_protocol_mismatch_reply(stream: &mut impl Write, client: u16) {
    best_effort_error_reply(
        stream,
        ServerError::ProtocolMismatch {
            client,
            server: PROTOCOL_VERSION,
        },
    );
}

#[cfg(windows)]
fn best_effort_error_reply(stream: &mut impl Write, error: ServerError) {
    let message = ProtocolMessage::CommandResponse(CommandResponse::Error {
        request_id: 0,
        error,
        output: RawText::default(),
    });
    let mut frame = Vec::new();
    if encode_protocol_message_into(&message, &mut frame).is_ok() {
        let _ = stream.write_all(&frame).and_then(|()| stream.flush());
    }
}

fn server_stopping_response(request_id: u64) -> ProtocolMessage {
    ProtocolMessage::CommandResponse(CommandResponse::Error {
        request_id,
        error: ServerError::InvalidCommand("server exited unexpectedly".to_owned()),
        output: RawText::default(),
    })
}

#[cfg(windows)]
fn best_effort_server_stopping_reply(stream: &mut impl Write) {
    let mut frame = Vec::new();
    if encode_protocol_message_into(&server_stopping_response(0), &mut frame).is_ok() {
        let _ = stream.write_all(&frame).and_then(|()| stream.flush());
    }
}

#[cfg(all(unix, test))]
fn write_outbound(
    stream: &mut impl TransportStream,
    outbound: &OutboundMailbox,
    shared: &Weak<Shared>,
    client: ClientId,
) {
    event_loop::write_fixture(stream, outbound, shared, client);
}

#[cfg(windows)]
fn write_outbound(
    stream: &mut impl TransportStream,
    outbound: &OutboundMailbox,
    shared: &Weak<Shared>,
    client: ClientId,
) {
    let _finished = OutboundWriterGuard(outbound);
    let mut batch = Vec::new();
    loop {
        if !outbound.recv_batch(&mut batch, attach::MAX_BATCHED_WRITE_BYTES) {
            break;
        }
        let started = diagnostic_timer();
        let frames = batch.len();
        let bytes = batch.iter().map(OutboundFrame::len).sum::<usize>();
        let result = attach::write_frames(stream, &batch);
        log::trace!(
            target: "zz_daemon::diagnostics::outbound",
            "write bytes={bytes} frames={frames} success={} elapsed_us={}",
            result.is_ok(),
            diagnostic_elapsed_us(started),
        );
        if result.is_err() {
            outbound.close();
            break;
        }
        outbound.finish_batch(&mut batch);
        if let Some(shared) = shared.upgrade() {
            shared.send_preview_refreshes(client, outbound);
        }
    }
    let _ = stream.shutdown();
}

fn validate_hello(hello: &ClientHello) -> Result<(), ServerError> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(ServerError::ProtocolMismatch {
            client: hello.protocol_version,
            server: PROTOCOL_VERSION,
        });
    }
    if hello.revision < zz_protocol::MIN_CLIENT_REVISION {
        return Err(ServerError::ClientTooOld {
            client: hello.revision,
            minimum: zz_protocol::MIN_CLIENT_REVISION,
        });
    }
    Ok(())
}

fn daemon_server_error(error: DaemonError) -> ServerError {
    match error {
        DaemonError::Server(error) => error,
        DaemonError::InsertedCommandParse(message) => ServerError::CommandParse(message),
        DaemonError::CommandFailed { error, .. } => daemon_server_error(*error),
        DaemonError::CommandExit { exit_code, .. }
        | DaemonError::ReportedCommandExit { exit_code, .. } => {
            ServerError::Internal(format!("command exited with status {exit_code}"))
        }
        other => ServerError::Internal(other.to_string()),
    }
}

fn post_admission_callback_error(error: DaemonError) -> DaemonError {
    match error {
        error @ (DaemonError::CommandExit { .. } | DaemonError::ReportedCommandExit { .. }) => {
            error
        }
        DaemonError::CommandFailed { output, error } => DaemonError::CommandFailed {
            output,
            error: Box::new(DaemonError::Server(ServerError::PostAdmissionCallback(
                Box::new(daemon_server_error(*error)),
            ))),
        },
        error => DaemonError::Server(ServerError::PostAdmissionCallback(Box::new(
            daemon_server_error(error),
        ))),
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct StartLock {
    path: PathBuf,
    _file: fs::File,
}

#[cfg(unix)]
fn start_lock_path(socket: &Path) -> PathBuf {
    let mut path = socket.as_os_str().to_owned();
    path.push(".lock");
    PathBuf::from(path)
}

#[cfg(unix)]
impl StartLock {
    #[allow(
        unsafe_code,
        reason = "the restarting daemon handed this descriptor over exec and nothing else owns it"
    )]
    fn adopt(path: PathBuf, fd: i32) -> Option<Self> {
        use std::os::{
            fd::{BorrowedFd, FromRawFd as _},
            unix::fs::MetadataExt as _,
        };

        if fd < 0 {
            return None;
        }
        let held = fs::File::from(
            unsafe { BorrowedFd::borrow_raw(fd) }
                .try_clone_to_owned()
                .ok()?,
        )
        .metadata()
        .ok()?;
        let current = fs::metadata(&path).ok()?;
        if (held.dev(), held.ino()) != (current.dev(), current.ino()) {
            return None;
        }
        let file = unsafe { fs::File::from_raw_fd(fd) };
        rustix::io::fcntl_setfd(&file, rustix::io::FdFlags::CLOEXEC).ok()?;
        Some(Self { path, _file: file })
    }

    fn acquire(path: PathBuf) -> Result<Self, DaemonError> {
        use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};

        loop {
            let file = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .mode(0o600)
                .open(&path)?;
            rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
                .map_err(std::io::Error::from)?;
            let held = file.metadata()?;
            match fs::metadata(&path) {
                Ok(current) if current.dev() == held.dev() && current.ino() == held.ino() => {
                    return Ok(Self { path, _file: file });
                }
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

#[cfg(unix)]
impl Drop for StartLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
fn prepare_socket(
    path: &Path,
    ready: &mut BootstrapReady,
    handed_lock: Option<i32>,
) -> Result<StartLock, DaemonError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let lock_path = start_lock_path(path);
    let lock = match handed_lock.and_then(|fd| StartLock::adopt(lock_path.clone(), fd)) {
        Some(lock) => lock,
        None => StartLock::acquire(lock_path)?,
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while path.exists() {
        match daemon_answers_at(path) {
            Some(true) => {
                ready.signal();
                return Err(DaemonError::AlreadyRunning(path.to_owned()));
            }
            Some(false) if Instant::now() >= deadline => {
                return Err(DaemonError::AlreadyRunning(path.to_owned()));
            }
            Some(false) => thread::sleep(Duration::from_millis(20)),
            None => {
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                break;
            }
        }
    }
    Ok(lock)
}

#[cfg(unix)]
fn daemon_answers_at(path: &Path) -> Option<bool> {
    let mut stream = LocalTransport::connect(path).ok()?;
    let (answered, answer) = crossbeam_channel::bounded(1);
    let probe = thread::Builder::new()
        .name("zz-socket-probe".to_owned())
        .spawn(move || {
            let request = ProtocolMessage::Exec(zz_protocol::ExecRequest {
                protocol_version: PROTOCOL_VERSION,
                flags: zz_protocol::ExecFlags::default(),
                client_instance_id: ClientInstanceId::default(),
                origin: None,
                working_directory: None,
                tty: None,
                size: None,
                features: 0,
                startup_reentry: None,
                spawned_server_id: None,
                expect_server_id: None,
                process_id: std::process::id(),
                environment: ClientEnvironmentBlob::default(),
                commands: Vec::new(),
                raw_control_line: None,
            });
            let live = zz_protocol::write_protocol_message(&mut stream, &request).is_ok()
                && matches!(
                    zz_protocol::read_protocol_message(&mut stream),
                    Ok(ProtocolMessage::ExecExit(_))
                );
            let _ = answered.send(live);
        });
    if probe.is_err() {
        return Some(false);
    }
    Some(
        answer
            .recv_timeout(Duration::from_millis(250))
            .unwrap_or(false),
    )
}

#[cfg(windows)]
fn prepare_socket(_: &Path, _: &mut BootstrapReady) -> Result<(), DaemonError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_socket_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(windows)]
fn restrict_socket_permissions(_: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

fn server_id() -> u64 {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let folded = u64::try_from(timestamp & u128::from(u64::MAX)).unwrap_or_default();
    folded ^ u64::from(std::process::id())
}

fn expand_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        return home_directory().map_or_else(|| PathBuf::from(path), |home| home.join(rest));
    }
    PathBuf::from(path)
}

#[cfg(unix)]
fn source_working_directory_glob_prefix(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};

    let bytes = path.as_os_str().as_bytes();
    let mut quoted = Vec::with_capacity(bytes.len().saturating_mul(2));
    for &byte in bytes {
        if byte.is_ascii() && !byte.is_ascii_alphanumeric() && byte != b'/' {
            quoted.push(b'\\');
        }
        quoted.push(byte);
    }
    PathBuf::from(std::ffi::OsString::from_vec(quoted))
}

#[cfg(not(unix))]
fn source_working_directory_glob_prefix(path: &Path) -> PathBuf {
    PathBuf::from(glob::Pattern::escape(path.to_string_lossy().as_ref()))
}

#[cfg(test)]
fn literal_directory_name(prefix: &str) -> String {
    let specials = if cfg!(windows) {
        "[literal]"
    } else {
        "[literal]*?"
    };
    format!("{prefix} {specials} with spaces")
}

fn resolve_source_path(path: &RawText, working_directory: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(path.to_os_string());
    if path.is_absolute() {
        path
    } else {
        working_directory.map_or(path.clone(), |working_directory| {
            source_working_directory_glob_prefix(working_directory).join(path)
        })
    }
}

fn prompt_history_path(path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        None
    } else if Path::new(path).is_absolute() {
        Some(PathBuf::from(path))
    } else {
        path.strip_prefix("~/")
            .and_then(|rest| home_directory().map(|home| home.join(rest)))
    }
}

fn load_command_prompt_history(path: &Path, limit: usize) -> (Vec<String>, Vec<String>) {
    let Ok(contents) = fs::read_to_string(path) else {
        return (Vec::new(), Vec::new());
    };
    let mut command = Vec::new();
    let mut search = Vec::new();
    for line in contents.split('\n').filter(|line| !line.is_empty()) {
        if let Some(entry) = line.strip_prefix("command:") {
            add_command_prompt_history(&mut command, entry, limit);
        } else if let Some(entry) = line.strip_prefix("search:") {
            add_command_prompt_history(&mut search, entry, limit);
        } else {
            add_command_prompt_history(&mut command, line, limit);
        }
    }
    (command, search)
}

fn add_command_prompt_history(history: &mut Vec<String>, input: &str, limit: usize) -> bool {
    let added = history.last().is_none_or(|entry| entry != input);
    if added {
        history.push(input.to_owned());
    }
    let removed = history.len().saturating_sub(limit);
    if history.len() > limit {
        history.drain(..removed);
    }
    removed != 0 || (added && limit != 0)
}

fn save_command_prompt_history(path: &Path, command: &[String], search: &[String]) {
    let mut contents = String::new();
    for entry in command {
        let _ = writeln!(contents, "command:{entry}");
    }
    for entry in search {
        let _ = writeln!(contents, "search:{entry}");
    }
    if let Err(error) = fs::write(path, contents) {
        log::debug!("failed to save prompt history {}: {error}", path.display());
    }
}

#[derive(Default)]
struct SourceGlobMatches {
    paths: Vec<PathBuf>,
    errors: Vec<String>,
}

const STANDARD_INPUT_SOURCE_WARNING: &str = "source-file from standard input is not supported";

fn spent_source_stream_error() -> String {
    source_glob_error_warning(Path::new("-"), "Bad file descriptor")
}

fn caller_stdin_read_failure(message: &str) -> bool {
    message == spent_source_stream_error()
        || message == source_glob_error_warning(Path::new("-"), "Input/output error")
}

fn source_glob_error_warning(path: &Path, error: &str) -> String {
    format!("{error}: {}", path.display())
}

fn filesystem_error_message(error: &std::io::Error) -> String {
    let message = error.to_string();
    error.raw_os_error().map_or(message.clone(), |code| {
        #[cfg(not(unix))]
        if let Some(text) = posix_error_text(error.kind()) {
            return text.to_owned();
        }
        message
            .strip_suffix(&format!(" (os error {code})"))
            .unwrap_or(&message)
            .to_owned()
    })
}

#[cfg(not(unix))]
fn posix_error_text(kind: ErrorKind) -> Option<&'static str> {
    Some(match kind {
        ErrorKind::NotFound => "No such file or directory",
        ErrorKind::PermissionDenied => "Permission denied",
        ErrorKind::IsADirectory => "Is a directory",
        ErrorKind::NotADirectory => "Not a directory",
        ErrorKind::InvalidInput | ErrorKind::InvalidFilename => "Invalid argument",
        ErrorKind::OutOfMemory => "Cannot allocate memory",
        _ => return None,
    })
}

fn source_read_error_warning(path: &Path, error: &std::io::Error) -> String {
    source_glob_error_warning(path, &filesystem_error_message(error))
}

fn client_source_read_error_warning(
    command_client: bool,
    path: &Path,
    error: &std::io::Error,
) -> String {
    if command_client && error.kind() == ErrorKind::IsADirectory {
        return source_glob_error_warning(path, "Input/output error");
    }
    source_read_error_warning(path, error)
}

fn missing_source_error(path: &Path) -> String {
    format!("No such file or directory: {}", path.display())
}

#[cfg(unix)]
#[allow(unsafe_code, clippy::undocumented_unsafe_blocks)]
fn source_glob_matches(path: &Path) -> SourceGlobMatches {
    use std::{
        ffi::{CStr, CString},
        mem::MaybeUninit,
        os::unix::ffi::{OsStrExt as _, OsStringExt as _},
    };

    let error_message = |errno| {
        unsafe { CStr::from_ptr(libc::strerror(errno)) }
            .to_string_lossy()
            .into_owned()
    };
    let Ok(pattern) = CString::new(path.as_os_str().as_bytes()) else {
        return SourceGlobMatches {
            paths: Vec::new(),
            errors: vec![error_message(libc::EINVAL)],
        };
    };
    let mut state = MaybeUninit::<libc::glob_t>::zeroed();
    let result = unsafe { libc::glob(pattern.as_ptr(), 0, None, state.as_mut_ptr()) };
    let mut state = unsafe { state.assume_init() };
    let mut matches = SourceGlobMatches::default();
    if result == 0 {
        for index in 0..state.gl_pathc {
            let path = unsafe { *state.gl_pathv.add(index) };
            if !path.is_null() {
                let bytes = unsafe { CStr::from_ptr(path) }.to_bytes().to_vec();
                matches
                    .paths
                    .push(PathBuf::from(std::ffi::OsString::from_vec(bytes)));
            }
        }
    } else if result == libc::GLOB_NOSPACE {
        matches.errors.push(error_message(libc::ENOMEM));
    } else if result != libc::GLOB_NOMATCH {
        matches.errors.push(error_message(libc::EINVAL));
    }
    unsafe { libc::globfree(&raw mut state) };
    matches
}

#[cfg(not(unix))]
fn source_glob_matches(path: &Path) -> SourceGlobMatches {
    match glob::glob(path.to_string_lossy().as_ref()) {
        Ok(paths) => {
            let mut matches = SourceGlobMatches::default();
            for path in paths {
                match path {
                    Ok(path) => matches.paths.push(path),
                    Err(error) => matches.errors.push(error.to_string()),
                }
            }
            matches.paths.sort();
            matches
        }
        Err(_) => SourceGlobMatches {
            paths: path
                .exists()
                .then(|| path.to_path_buf())
                .into_iter()
                .collect(),
            errors: Vec::new(),
        },
    }
}

#[cfg(test)]
fn tmux_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "\\\\")
}

#[cfg(test)]
mod tests;
