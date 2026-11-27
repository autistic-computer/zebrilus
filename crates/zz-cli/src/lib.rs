#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        reason = "tests spawn helper processes from threads with an empty signal mask"
    )
)]

#[cfg(not(windows))]
extern crate mimalloc;
#[cfg(test)]
use zz_daemon::{CommandStdinSink, append_stdin_payload};
mod agent_hooks;
mod agent_login;
mod command_mode;
mod control_mode;
mod copy_file;
mod daemon_spawn;
pub mod diagnostics;
mod events;
mod fleet;
mod fork_chat;
mod help;
mod host_cli;
mod kill_server;
mod launcher;
mod list_hosts;
mod native_attach;
mod output;
mod startup;
mod tailnet;
pub mod update;
use command_mode::*;
pub use daemon_spawn::connect_interactive_client_with_config;
pub use daemon_spawn::connect_interactive_client_with_config_and_terminal;
pub use daemon_spawn::connect_or_spawn_daemon;
pub use daemon_spawn::connect_or_spawn_daemon_with_provenance;
pub use daemon_spawn::connect_terminal_surface_client_with_config;
pub use daemon_spawn::daemon_executable;
use daemon_spawn::*;
use help::*;
use host_cli::*;
use kill_server::*;
pub use launcher::launch_application;
use launcher::*;
use native_attach::*;
use output::*;
pub use startup::application_arguments;
#[cfg(target_os = "windows")]
pub use startup::attach_parent_console;
#[cfg(any(unix, windows))]
pub use startup::run_askpass_mode;
pub use startup::run_startup;
#[cfg(target_os = "macos")]
pub use startup::tag_allocator_memory;
use startup::*;

#[cfg(test)]
use std::borrow::Cow;
use std::{
    cell::RefCell,
    io::{self, ErrorKind, IsTerminal as _, Write as _},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use zz_daemon::{Daemon, Stopped};
use zz_daemon_client::{
    CommandClient, CommandOutcome, DaemonError, Endpoint, ExecChain, ExecChainEnd, ExecClassifier,
    InteractiveClient, classify_local_connect_error, terminate_incompatible_daemon,
    unmasked::SpawnUnmasked as _,
};
use zz_mux::MuxEngine;
use zz_protocol::{
    CommandInvocation, ExecResume, ExecResumeKind, MAX_CLIENT_WORKING_DIRECTORY_BYTES,
    PROTOCOL_VERSION, PreparedCommand, RawText, ServerError, ServerHello, StdoutClaim,
    canonical_command, catalog_command_spec,
};
use zz_terminal::TerminalColorScheme;

const FOREIGN_TMUX_ERROR: &str = "zz: TMUX is set but ZZ_SOCKET is not; refusing to treat a tmux server as zz\nUse `zz app` to open the GUI, or pass `-S` / set `ZZ_SOCKET` to target a zz daemon.";
pub const APP_STARTUP_DIRECTORY_ENV: &str = "ZZ_APP_STARTUP_DIRECTORY";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandLineOrigin {
    Launcher,
    Application,
}

pub type BrowserProvider = fn() -> Option<Box<dyn zz_tui::browser::BrowserFrameProvider>>;

#[derive(Clone, Copy)]
pub struct StartupOptions {
    pub origin: CommandLineOrigin,
    pub browser_provider: Option<BrowserProvider>,
}

const DAEMON_BOOTSTRAP_SERVER_ID_ARGUMENT: &str = "--bootstrap-server-id";
const DAEMON_BOOTSTRAP_READY_FD_ARGUMENT: &str = "--bootstrap-ready-fd";
const DAEMON_BOOTSTRAP_CLIENT_CWD_ARGUMENT: &str = "--bootstrap-client-cwd";
#[cfg(unix)]
const DAEMON_START_LOCK_FD_ARGUMENT: &str = "--start-lock-fd";
#[cfg(windows)]
const DAEMON_RETRY_BIND_ARGUMENT: &str = "--retry-bind";

pub enum Startup {
    Application(PathBuf),
    Exit(ExitCode),
}

#[derive(Debug, Default, PartialEq, Eq)]
struct DaemonBootstrapArguments {
    server_id: Option<u64>,
    ready_fd: Option<i32>,
    client_working_directory: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
enum DaemonBootstrapArgumentError {
    ServerId,
    ReadyFd,
    ClientWorkingDirectory,
}

impl DaemonBootstrapArgumentError {
    fn message(&self) -> &'static str {
        match self {
            Self::ServerId => "invalid bootstrap server id",
            Self::ReadyFd => "invalid bootstrap ready fd",
            Self::ClientWorkingDirectory => "invalid bootstrap client cwd",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct NativeAttachArguments {
    restart_daemon: bool,
    detach_others: bool,
    detach_others_hangup: bool,
    no_update_environment: bool,
    read_only: bool,
    client_flags: Option<String>,
    working_directory: Option<String>,
    session: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum NativeAttachArgumentError {
    Usage,
    NativeUsage,
    Command(ServerError),
}

struct TmuxLabelCreationError {
    message: String,
    kind: ErrorKind,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ApplicationArguments {
    pub socket_path: PathBuf,
    socket_source: SocketSelectionSource,
    host: Option<String>,
    remaining: Vec<RawText>,
    mux_config_files: Vec<PathBuf>,
    no_start_server: bool,
    control_mode: u8,
    shell_command: Option<String>,
    login_shell: bool,
    early_output: Option<&'static str>,
    client_utf8: bool,
    client_features: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApplicationArgumentError {
    Message(String),
    Raw(String),
    Usage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SocketSelectionSource {
    Default,
    Path,
    Label,
}

impl SocketSelectionSource {
    fn is_overridden(self) -> bool {
        self != Self::Default
    }
}

struct ChainRouting<'a> {
    socket_path: &'a Path,
    host: Option<&'a str>,
    mux_config_files: &'a [PathBuf],
    startup_options: &'a StartupOptions,
    command: &'a str,
    start_server: bool,
}

thread_local! {
    static COMMAND_HOST: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[cfg(test)]
mod command_mode_tests;
#[cfg(test)]
mod daemon_spawn_tests;
#[cfg(test)]
mod host_cli_tests;
#[cfg(test)]
mod kill_server_tests;
#[cfg(test)]
mod launcher_tests;
#[cfg(test)]
mod native_attach_tests;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod startup_tests;
