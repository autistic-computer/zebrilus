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
mod tailnet;
pub mod update;
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

const TMUX_VERSION_OUTPUT: &str = zz_protocol::CommandSpec::TMUX_VERSION_OUTPUT;
const TMUX_USAGE: &str = concat!(
    "usage: zz [-2CDhlNuVv] [-c shell-command] [-f file] [-L socket-name]\n",
    "            [-S socket-path] [-T features] [command [flags]]"
);
const NATIVE_APP_USAGE: &str = "zz: usage: zz app";
const ALL_HOSTS: &str = "all";
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
static HANDED_OVER_FROM: std::sync::OnceLock<String> = std::sync::OnceLock::new();

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

#[must_use]
pub fn run_startup(socket_path: &Path, options: StartupOptions) -> Startup {
    #[cfg(unix)]
    let handover_failure = if options.origin == CommandLineOrigin::Launcher {
        hand_over_to_owned_copy()
    } else {
        None
    };
    diagnostics::init(bare_command_line_opens_application(
        options.origin,
        launched_by_launch_services(),
    ));
    #[cfg(unix)]
    if let Some((binary, error)) = handover_failure {
        log::warn!(
            target: "zz::diagnostics::process",
            "could not hand over to the owned copy path={} error={error}",
            binary.display(),
        );
    }
    let arguments =
        match application_arguments(diagnostics::application_args(), socket_path.to_path_buf()) {
            Ok(arguments) => arguments,
            Err(ApplicationArgumentError::Message(error)) => {
                eprintln!("zz: {error}");
                return Startup::Exit(exit_code_for(CliFailure::Usage));
            }
            Err(ApplicationArgumentError::Raw(error)) => {
                eprintln!("{error}");
                return Startup::Exit(exit_code_for(CliFailure::TmuxUsage));
            }
            Err(ApplicationArgumentError::Usage) => {
                eprintln!("{TMUX_USAGE}");
                return Startup::Exit(exit_code_for(CliFailure::TmuxUsage));
            }
        };
    let ApplicationArguments {
        socket_path,
        socket_source,
        host,
        remaining,
        mux_config_files,
        no_start_server,
        control_mode,
        shell_command,
        login_shell,
        early_output,
        client_utf8,
        client_features,
    } = arguments;
    let host = host.filter(|name| !names_this_machine(name));
    zz_daemon_client::set_client_terminal_flags(zz_daemon_client::ClientTerminalFlags {
        utf8: client_utf8,
        features: client_features,
    });
    let implicit_tmux_conflict = implicit_tmux_endpoint_conflict(
        socket_source,
        std::env::var_os("ZZ_SOCKET").as_deref(),
        std::env::var_os("TMUX").as_deref(),
    );
    if let Some(output) = early_output {
        println!("{output}");
        return Startup::Exit(ExitCode::SUCCESS);
    }
    if control_mode != 0 && shell_command.is_none() {
        if let Some(host) = host.as_deref() {
            return Startup::Exit(match configured_host_endpoint(host) {
                Ok(endpoint) => control_mode::run_host(host, &endpoint, control_mode, &remaining),
                Err(error) => {
                    eprintln!("zz: {error}");
                    exit_code_for(CliFailure::Runtime)
                }
            });
        }
        if implicit_tmux_conflict {
            eprintln!("{FOREIGN_TMUX_ERROR}");
            return Startup::Exit(exit_code_for(CliFailure::Runtime));
        }
        return Startup::Exit(control_mode::run(
            &socket_path,
            socket_source,
            &mux_config_files,
            no_start_server,
            control_mode,
            &remaining,
        ));
    }
    if let Some(exit) = run_command_mode(
        &remaining,
        &socket_path,
        socket_source,
        host.as_deref(),
        &mux_config_files,
        no_start_server,
        shell_command.as_deref(),
        login_shell,
        &options,
        implicit_tmux_conflict,
    ) {
        Startup::Exit(exit)
    } else {
        if options.origin == CommandLineOrigin::Application
            || std::env::args_os()
                .skip(1)
                .ne([std::ffi::OsString::from("app")])
        {
            configure_application_working_directory();
        }
        Startup::Application(socket_path)
    }
}

#[cfg(unix)]
fn hand_over_to_owned_copy() -> Option<(PathBuf, io::Error)> {
    use std::os::unix::process::CommandExt as _;
    use zz_config::owned::{self, BuildInfo, Launch};

    let launch = Launch::from_env(
        std::env::args_os().any(|argument| argument == DAEMON_BOOTSTRAP_SERVER_ID_ARGUMENT),
    );
    if let Some(packaged) = std::env::var_os(owned::HANDOVER_ENV) {
        #[allow(
            unsafe_code,
            reason = "startup runs on one thread and nothing has read the environment yet"
        )]
        unsafe {
            std::env::remove_var(owned::HANDOVER_ENV);
        }
        let _ = HANDED_OVER_FROM.set(packaged.to_string_lossy().into_owned());
    }
    let this = BuildInfo::current();
    let copy = owned::installed()?;
    if !owned::hands_over(&this, &copy.info, launch) {
        return None;
    }
    let mut arguments = std::env::args_os();
    let mut command = Command::new(&copy.binary);
    if let Some(program) = arguments.next() {
        command.arg0(program);
    }
    let error = command
        .args(arguments)
        .env(owned::HANDOVER_ENV, this.version.to_string())
        .exec();
    Some((copy.binary, error))
}

pub(crate) fn handed_over_from() -> Option<&'static str> {
    HANDED_OVER_FROM.get().map(String::as_str)
}

fn application_working_directory(
    launched: Option<&Path>,
    current: Option<&Path>,
    home: Option<&Path>,
    home_from_root: bool,
) -> Option<PathBuf> {
    launched
        .filter(|directory| directory.is_dir())
        .or_else(|| {
            current.filter(|directory| {
                directory.is_dir() && (!home_from_root || *directory != Path::new("/"))
            })
        })
        .or_else(|| home.filter(|directory| directory.is_dir()))
        .or_else(|| current.filter(|directory| directory.is_dir()))
        .map(Path::to_owned)
}

fn configure_application_working_directory() {
    let launched = std::env::var_os(APP_STARTUP_DIRECTORY_ENV).map(PathBuf::from);
    let current = std::env::current_dir().ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let selected = application_working_directory(
        launched.as_deref(),
        current.as_deref(),
        home.as_deref(),
        cfg!(target_os = "macos"),
    );
    let Some(selected) = selected.filter(|selected| current.as_deref() != Some(selected.as_path()))
    else {
        return;
    };
    if let Err(error) = std::env::set_current_dir(&selected) {
        log::warn!(
            target: "zz::diagnostics::process",
            "could not use application working directory path={} error={error}",
            selected.display(),
        );
    }
}
#[cfg(not(windows))]
const MI_OPTION_PURGE_DELAY: std::ffi::c_int = 15;
#[cfg(target_os = "macos")]
const MI_OPTION_OS_TAG: std::ffi::c_int = 18;
#[cfg(target_os = "macos")]
const MI_DEFAULT_OS_TAG: std::ffi::c_long = 100;
#[cfg(target_os = "macos")]
const ALLOCATOR_OS_TAG: std::ffi::c_long = 241;
#[cfg(not(windows))]
const DAEMON_PURGE_DELAY_MS: std::ffi::c_long = 0;

#[cfg(not(windows))]
#[allow(unsafe_code)]
unsafe extern "C" {
    #[cfg(target_os = "macos")]
    fn mi_option_get(option: std::ffi::c_int) -> std::ffi::c_long;
    fn mi_option_set(option: std::ffi::c_int, value: std::ffi::c_long);
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code, clippy::undocumented_unsafe_blocks)]
pub extern "C" fn tag_allocator_memory() {
    if unsafe { mi_option_get(MI_OPTION_OS_TAG) } == MI_DEFAULT_OS_TAG {
        unsafe { mi_option_set(MI_OPTION_OS_TAG, ALLOCATOR_OS_TAG) };
    }
}

#[cfg(not(windows))]
#[allow(unsafe_code, clippy::undocumented_unsafe_blocks)]
fn purge_freed_memory_promptly() {
    if std::env::var_os("MIMALLOC_PURGE_DELAY").is_none() {
        unsafe { mi_option_set(MI_OPTION_PURGE_DELAY, DAEMON_PURGE_DELAY_MS) };
    }
}

#[cfg(target_os = "windows")]
pub fn attach_parent_console() {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    #[allow(
        unsafe_code,
        reason = "AttachConsole is a raw Win32 entry point with no safe wrapper"
    )]
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(any(unix, windows))]
#[must_use]
pub fn run_askpass_mode() -> Option<ExitCode> {
    let socket = std::env::var_os(zz_daemon_client::ASKPASS_SOCKET_ENV)?;
    let prompt = std::env::args_os().nth(1).unwrap_or_default();
    Some(zz_daemon_client::run_helper(
        Path::new(&socket),
        &prompt.to_string_lossy(),
    ))
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

enum SocketSelection {
    Path(PathBuf),
    Label(String),
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

pub fn application_arguments(
    arguments: impl IntoIterator<Item = RawText>,
    default_path: PathBuf,
) -> Result<ApplicationArguments, ApplicationArgumentError> {
    let mut socket_selection = None;
    let mut host = None;
    let mut remaining = Vec::new();
    let mut mux_config_files = Vec::new();
    let mut no_start_server = false;
    let mut shell_command = None;
    let mut login_shell = false;
    let mut control_mode = 0_u8;
    let mut foreground_server = false;
    let mut client_utf8 = false;
    let mut client_features = Vec::new();
    let mut parsing_tmux_options = true;
    let mut verb_takes_socket = false;
    let mut arguments = arguments.into_iter().collect::<Vec<_>>().into_iter();
    while let Some(argument) = arguments.next() {
        let socket_flag = parsing_tmux_options || verb_takes_socket;
        if socket_flag && argument == diagnostics::SOCKET_ARGUMENT {
            let path = arguments.next().ok_or_else(|| {
                ApplicationArgumentError::Message("--socket requires a path".to_owned())
            })?;
            if path.is_empty() {
                return Err(ApplicationArgumentError::Message(
                    "--socket requires a non-empty path".to_owned(),
                ));
            }
            socket_selection = Some(SocketSelection::Path(PathBuf::from(path.to_os_string())));
        } else if let Some(path) = argument
            .strip_prefix(diagnostics::SOCKET_ARGUMENT)
            .and_then(|argument| argument.strip_prefix('='))
            .filter(|_| socket_flag)
        {
            if path.is_empty() {
                return Err(ApplicationArgumentError::Message(
                    "--socket requires a non-empty path".to_owned(),
                ));
            }
            socket_selection = Some(SocketSelection::Path(PathBuf::from(path)));
        } else if parsing_tmux_options && argument == "--host" {
            let name = arguments.next().ok_or_else(|| {
                ApplicationArgumentError::Message("--host requires a name".to_owned())
            })?;
            if name.is_empty() {
                return Err(ApplicationArgumentError::Message(
                    "--host requires a non-empty name".to_owned(),
                ));
            }
            host = Some(name.to_string());
        } else if let Some(name) = argument
            .strip_prefix("--host=")
            .filter(|_| parsing_tmux_options)
        {
            if name.is_empty() {
                return Err(ApplicationArgumentError::Message(
                    "--host requires a non-empty name".to_owned(),
                ));
            }
            host = Some(name.into());
        } else if parsing_tmux_options && argument == "--" {
            parsing_tmux_options = false;
        } else if parsing_tmux_options
            && matches!(argument.as_str(), "--version" | "--kill-server" | "--help")
        {
            parsing_tmux_options = false;
            remaining.push(argument);
        } else if parsing_tmux_options && argument.starts_with("--") {
            return Err(ApplicationArgumentError::Usage);
        } else if parsing_tmux_options && argument.starts_with('-') && argument != "-" {
            let options = &argument[1..];
            for (index, option) in options.char_indices() {
                let value = |arguments: &mut std::vec::IntoIter<RawText>| {
                    let value_index = index + option.len_utf8();
                    if value_index < options.len() {
                        Ok(RawText::from(&options[value_index..]))
                    } else {
                        arguments.next().ok_or_else(|| {
                            ApplicationArgumentError::Raw(format!(
                                "zz: option requires an argument -- {option}\n{TMUX_USAGE}"
                            ))
                        })
                    }
                };
                match option {
                    '2' => client_features.push("256".to_owned()),
                    'q' | 'v' => {}
                    'u' => client_utf8 = true,
                    'c' => {
                        shell_command = Some(value(&mut arguments)?.to_string());
                        break;
                    }
                    'C' => control_mode = control_mode.saturating_add(1),
                    'D' => foreground_server = true,
                    'f' => {
                        mux_config_files.push(PathBuf::from(value(&mut arguments)?.to_os_string()));
                        break;
                    }
                    'h' => {
                        return Ok(ApplicationArguments {
                            socket_path: default_path,
                            socket_source: SocketSelectionSource::Default,
                            host: None,
                            remaining: Vec::new(),
                            mux_config_files: Vec::new(),
                            no_start_server: false,
                            control_mode: 0,
                            shell_command: None,
                            login_shell: false,
                            client_utf8: false,
                            client_features: Vec::new(),
                            early_output: Some(TMUX_USAGE),
                        });
                    }
                    'l' => login_shell = true,
                    'L' => {
                        socket_selection =
                            Some(SocketSelection::Label(value(&mut arguments)?.to_string()));
                        break;
                    }
                    'N' => no_start_server = true,
                    'S' => {
                        socket_selection = Some(SocketSelection::Path(PathBuf::from(
                            value(&mut arguments)?.to_os_string(),
                        )));
                        break;
                    }
                    'T' => {
                        client_features.push(value(&mut arguments)?.to_string());
                        break;
                    }
                    'V' => {
                        return Ok(ApplicationArguments {
                            socket_path: default_path,
                            socket_source: SocketSelectionSource::Default,
                            host: None,
                            remaining: Vec::new(),
                            mux_config_files: Vec::new(),
                            no_start_server: false,
                            control_mode: 0,
                            shell_command: None,
                            login_shell: false,
                            client_utf8: false,
                            client_features: Vec::new(),
                            early_output: Some(TMUX_VERSION_OUTPUT),
                        });
                    }
                    _ => {
                        return Err(ApplicationArgumentError::Raw(format!(
                            "zz: unknown option -- {option}\n{TMUX_USAGE}"
                        )));
                    }
                }
            }
        } else {
            if parsing_tmux_options {
                verb_takes_socket = matches!(argument.as_str(), "daemon" | "proxy" | "app");
            } else if argument == "--" {
                verb_takes_socket = false;
            }
            parsing_tmux_options = false;
            remaining.push(argument);
        }
    }
    if shell_command.is_some() && !remaining.is_empty() {
        return Err(ApplicationArgumentError::Usage);
    }
    if foreground_server && !remaining.is_empty() {
        return Err(ApplicationArgumentError::Usage);
    }
    if foreground_server {
        return Err(ApplicationArgumentError::Message(
            "-D foreground server mode is not supported; use `zz daemon`".to_owned(),
        ));
    }
    if host.as_deref().is_some_and(|host| host.starts_with('-')) {
        return Err(ApplicationArgumentError::Message(
            "--host takes a host name or ssh destination, and it must not start with `-`"
                .to_owned(),
        ));
    }
    let socket_source = match &socket_selection {
        Some(SocketSelection::Path(_)) => SocketSelectionSource::Path,
        Some(SocketSelection::Label(_)) => SocketSelectionSource::Label,
        None => SocketSelectionSource::Default,
    };
    if socket_source.is_overridden() && host.is_some() {
        return Err(ApplicationArgumentError::Message(
            "--host cannot be used together with a socket selector".to_owned(),
        ));
    }
    let socket_path = match socket_selection {
        Some(SocketSelection::Path(path)) => path,
        Some(SocketSelection::Label(label)) => {
            tmux_label_socket_path(&label, std::env::var_os("TMUX_TMPDIR").as_deref())
                .map_err(ApplicationArgumentError::Raw)?
        }
        None => default_path,
    };
    Ok(ApplicationArguments {
        socket_path,
        socket_source,
        host,
        remaining,
        mux_config_files,
        no_start_server,
        control_mode,
        shell_command,
        login_shell,
        early_output: None,
        client_utf8,
        client_features,
    })
}

fn implicit_tmux_endpoint_conflict(
    socket_source: SocketSelectionSource,
    zz_socket: Option<&std::ffi::OsStr>,
    tmux: Option<&std::ffi::OsStr>,
) -> bool {
    socket_source == SocketSelectionSource::Default
        && zz_socket.is_none_or(std::ffi::OsStr::is_empty)
        && tmux.is_some_and(|value| !value.is_empty())
}

#[cfg(unix)]
fn tmux_label_socket_path(
    label: &str,
    tmux_tmpdir: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, String> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};

    let root = tmux_socket_root(tmux_tmpdir).ok_or_else(|| "no suitable socket path".to_owned())?;
    let uid = rustix::process::getuid().as_raw();
    let base = root.join(format!("tmux-{uid}"));
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    if let Err(error) = builder.create(&base)
        && error.kind() != ErrorKind::AlreadyExists
    {
        return Err(format!(
            "couldn't create directory {} ({error})",
            base.display()
        ));
    }
    let metadata = std::fs::symlink_metadata(&base)
        .map_err(|error| format!("couldn't read directory {} ({error})", base.display()))?;
    if !metadata.file_type().is_dir() {
        return Err(format!("{} is not a directory", base.display()));
    }
    if metadata.uid() != uid || metadata.mode() & 0o007 != 0 {
        return Err(format!(
            "directory {} has unsafe permissions",
            base.display()
        ));
    }
    Ok(base.join(label))
}

#[cfg(unix)]
fn tmux_socket_root(tmux_tmpdir: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    tmux_tmpdir
        .filter(|path| !path.is_empty())
        .and_then(|path| std::fs::canonicalize(Path::new(path)).ok())
        .or_else(|| std::fs::canonicalize("/tmp").ok())
}

#[cfg(not(unix))]
fn tmux_label_socket_path(
    label: &str,
    tmux_tmpdir: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, String> {
    let root = tmux_tmpdir
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let base = root.join("tmux-0");
    std::fs::create_dir_all(&base)
        .map_err(|error| format!("couldn't create directory {} ({error})", base.display()))?;
    Ok(base.join(label))
}

fn run_command_mode(
    arguments: &[RawText],
    socket_path: &Path,
    socket_source: SocketSelectionSource,
    host: Option<&str>,
    mux_config_files: &[PathBuf],
    no_start_server: bool,
    shell_command: Option<&str>,
    login_shell: bool,
    startup_options: &StartupOptions,
    implicit_tmux_conflict: bool,
) -> Option<ExitCode> {
    if let Some(shell_command) = shell_command {
        if implicit_tmux_conflict && host.is_none() {
            eprintln!("{FOREIGN_TMUX_ERROR}");
            return Some(exit_code_for(CliFailure::Runtime));
        }
        return Some(run_tmux_shell_command(
            socket_path,
            socket_source,
            host,
            mux_config_files,
            no_start_server,
            shell_command,
            login_shell,
        ));
    }
    let mut command_chain = split_command_chain(arguments);
    if command_chain.is_empty() && host.is_some_and(|host| host != ALL_HOSTS) {
        command_chain = vec![CommandInvocation::new("new-session", ["-A"])];
    }
    if command_chain.is_empty()
        && host.is_none()
        && !bare_command_line_opens_application(
            startup_options.origin,
            launched_by_launch_services(),
        )
    {
        command_chain =
            default_client_command_chain(socket_path, mux_config_files, no_start_server);
    }
    let Some(invocation) = command_chain.first().cloned() else {
        if host.is_some() {
            eprintln!(
                "zz: --host all needs a read-only command, such as zz --host all list-panes -a"
            );
            return Some(exit_code_for(CliFailure::Usage));
        }
        return None;
    };
    let command = invocation.name.clone();
    if command == "--help" || command == "help" {
        print!("{}", top_level_help());
        return Some(ExitCode::SUCCESS);
    }
    if let Some(host) = host
        && host != ALL_HOSTS
        && matches!(
            canonical_command(&command),
            "fleet" | "list-hosts" | "tools" | "protocol-version" | "build-info" | "--version"
        )
    {
        return Some(run_host_cli(host, arguments));
    }
    if invocation
        .args
        .iter()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--help")
    {
        return Some(match command_help(&command) {
            Ok(help) => {
                print!("{help}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{}", server_error_message(&error));
                exit_code_for(CliFailure::Server(&error))
            }
        });
    }
    if canonical_command(&command) == "tools" && host.is_none() && command_chain.len() == 1 {
        if let [flag, rest @ ..] = invocation.args.as_slice()
            && flag == "--agent-login"
        {
            return Some(agent_login::run(socket_path, rest));
        }
        return Some(agent_hooks::run(&invocation.args));
    }
    if command == "update" {
        if host == Some(ALL_HOSTS) {
            eprintln!("zz: zz update -a updates this machine and every host");
            return Some(exit_code_for(CliFailure::Usage));
        }
        let arguments = invocation
            .args
            .iter()
            .map(ToString::to_string)
            .chain(host.map(str::to_owned))
            .collect::<Vec<_>>();
        return Some(update::run(&arguments, socket_path));
    }
    if host == Some(ALL_HOSTS) {
        return Some(run_on_every_host(arguments, &command_chain));
    }
    if let Some(host) = host
        && let Some(message) = untargeted_host_input(host, &command_chain)
    {
        eprintln!("{message}");
        return Some(exit_code_for(CliFailure::Usage));
    }
    let progress_target = if command == "agent-send" {
        match events::progress_target(&invocation.args) {
            Ok(target) => target,
            Err(message) => {
                eprintln!("{message}");
                return Some(exit_code_for(CliFailure::Usage));
            }
        }
    } else {
        None
    };
    if command == "events" {
        if command_chain.len() != 1 {
            eprintln!("events: use a single command: zz events [-t target]");
            return Some(exit_code_for(CliFailure::Usage));
        }
        if let Some(host) = host {
            return Some(match configured_host_endpoint(host) {
                Ok(endpoint) => events::run_host(host, &endpoint, &invocation.args),
                Err(error) => {
                    eprintln!("zz: {error}");
                    exit_code_for(CliFailure::Runtime)
                }
            });
        }
        if implicit_tmux_conflict {
            eprintln!("{FOREIGN_TMUX_ERROR}");
            return Some(exit_code_for(CliFailure::Runtime));
        }
        return Some(events::run(
            socket_path,
            socket_source,
            mux_config_files,
            no_start_server,
            &invocation.args,
        ));
    }
    if command == "app" {
        if host.is_some() || !invocation.args.is_empty() || command_chain.len() != 1 {
            eprintln!("{NATIVE_APP_USAGE}");
            return Some(exit_code_for(CliFailure::Usage));
        }
        return None;
    }
    if is_version_command(&command) {
        println!("zz {}", env!("CARGO_PKG_VERSION"));
        return Some(ExitCode::SUCCESS);
    }
    if command == "protocol-version" {
        return Some(
            match protocol_version_output(
                invocation.args.iter().map(ToString::to_string),
                socket_source.is_overridden(),
            ) {
                Ok(version) => {
                    println!("{version}");
                    ExitCode::SUCCESS
                }
                Err(usage) => {
                    eprintln!("zz: {usage}");
                    exit_code_for(CliFailure::Usage)
                }
            },
        );
    }
    if command == "build-info" {
        if !invocation.args.is_empty() {
            eprintln!("zz: usage: zz build-info");
            return Some(exit_code_for(CliFailure::Usage));
        }
        println!("{}", zz_config::owned::BuildInfo::current());
        return Some(ExitCode::SUCCESS);
    }

    if host.is_some() && matches!(command.as_str(), "daemon" | "proxy") {
        eprintln!("zz: --host is not supported for `{command}`");
        return Some(exit_code_for(CliFailure::Usage));
    }

    if command == "daemon" {
        #[cfg(unix)]
        let handoff = match invocation.args.as_slice() {
            [flag, fd] if flag == DAEMON_START_LOCK_FD_ARGUMENT => fd.parse::<i32>().ok(),
            _ => None,
        };
        #[cfg(windows)]
        let handoff = matches!(
            invocation.args.as_slice(),
            [flag] if flag == DAEMON_RETRY_BIND_ARGUMENT
        );
        #[cfg(unix)]
        let handed_off = handoff.is_some();
        #[cfg(windows)]
        let handed_off = handoff;
        let bootstrap = if handed_off {
            DaemonBootstrapArguments::default()
        } else {
            match parse_daemon_bootstrap_arguments(&invocation.args) {
                Ok(bootstrap) => bootstrap,
                Err(error) => {
                    eprintln!("zz daemon: {}", error.message());
                    return Some(exit_code_for(CliFailure::Usage));
                }
            }
        };
        #[cfg(not(windows))]
        purge_freed_memory_promptly();
        let mut daemon =
            Daemon::new(socket_path).with_mux_config_files(mux_config_files.iter().cloned());
        if let Some(server_id) = bootstrap.server_id {
            daemon = daemon.with_server_id(server_id);
        }
        if let Some(client_working_directory) = bootstrap.client_working_directory {
            daemon = daemon.with_initial_client_working_directory(client_working_directory);
        }
        #[cfg(unix)]
        if let Some(fd) = bootstrap.ready_fd {
            daemon = daemon.with_bootstrap_ready_fd(fd);
        }
        #[cfg(unix)]
        if let Some(fd) = handoff {
            daemon = daemon.with_start_lock_fd(fd);
        }
        #[cfg(windows)]
        if handoff {
            daemon = daemon.with_bind_retry();
        }
        return Some(match daemon.serve() {
            Ok(Stopped::Exit) | Err(DaemonError::AlreadyRunning(_)) => ExitCode::SUCCESS,
            Ok(Stopped::Restart(restart)) => {
                restart_daemon(socket_path, mux_config_files, &restart)
            }
            Err(error) => {
                eprintln!("zz daemon: {error}");
                exit_code_for(CliFailure::Runtime)
            }
        });
    }

    if command == "proxy" {
        return Some(match zz_daemon_client::run_socket_proxy(socket_path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("zz proxy: {error}");
                exit_code_for(CliFailure::Runtime)
            }
        });
    }

    if command == "tailnet" {
        return Some(
            match tailnet::run(invocation.args.iter().map(ToString::to_string), socket_path) {
                Ok(output) => {
                    println!("{output}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("zz: {error}");
                    exit_code_for(CliFailure::Usage)
                }
            },
        );
    }

    if command == "copy-file" {
        let words = invocation
            .args
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>();
        return Some(match copy_file::run(&words) {
            Ok(output) => {
                println!("{output}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("copy-file: {error}");
                exit_code_for(if error.starts_with("usage:") {
                    CliFailure::Usage
                } else {
                    CliFailure::Runtime
                })
            }
        });
    }

    if command == "fleet" {
        return Some(
            match fleet::run(
                invocation
                    .args
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>(),
            ) {
                Ok(output) => {
                    if !output.is_empty() {
                        println!("{output}");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    if !error.is_empty() {
                        eprintln!("zz: {error}");
                    }
                    exit_code_for(
                        if error.starts_with("usage:")
                            || error == "ssh destination must not start with `-`"
                        {
                            CliFailure::Usage
                        } else {
                            CliFailure::Runtime
                        },
                    )
                }
            },
        );
    }

    if command == "list-hosts" && host.is_none() {
        return Some(list_hosts::run(
            &invocation
                .args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        ));
    }

    if implicit_tmux_conflict && host.is_none() {
        eprintln!("{FOREIGN_TMUX_ERROR}");
        return Some(exit_code_for(CliFailure::Runtime));
    }

    if command == "--kill-server" {
        return Some(match host {
            Some(host) => run_host_kill_server(host, invocation.args),
            None => run_kill_server(socket_path, invocation.args),
        });
    }

    let start_server = !no_start_server && tmux_command_starts_server(&command);
    let routing = ChainRouting {
        socket_path,
        host,
        mux_config_files,
        startup_options,
        command: &command,
        start_server,
    };
    if let Some(host) = host {
        return Some(run_host_command_chain(
            &routing,
            host,
            socket_source,
            command_chain,
            progress_target.as_deref(),
        ));
    }

    let mut resume = None;
    let failure = match CommandClient::connect(socket_path) {
        Ok(client) => {
            match run_local_command_chain(
                &routing,
                client,
                &command_chain,
                progress_target.as_deref(),
                None,
            ) {
                LocalChain::Done(exit) => return Some(exit),
                LocalChain::Resume(prepared, client) => {
                    resume = Some((prepared, *client));
                    None
                }
                LocalChain::Unanswered(error) => Some(error),
            }
        }
        Err(error) => Some(error),
    };
    let failure = failure.map(|error| classify_local_connect_error(socket_path, error));

    if failure.is_some() {
        let static_commands = if matches!(command.as_str(), "attach" | "attach-session") {
            if let Err(error) = parse_native_attach_arguments(command_chain[0].args.clone()) {
                return Some(print_native_attach_argument_error(error));
            }
            &command_chain[1..]
        } else {
            &command_chain
        };
        if let Err(error) = zz_mux::validate_static_command_chain(static_commands) {
            eprintln!("{}", server_error_message(&error));
            return Some(exit_code_for(CliFailure::Server(&error)));
        }
    }
    let failure = match failure {
        Some(error)
            if !start_server
                && tmux_command_starts_server(&command)
                && daemon_is_spawnable(&error) =>
        {
            eprintln!("{}", format_local_command_error(socket_path, error));
            return Some(exit_code_for(CliFailure::Runtime));
        }
        failure => failure,
    };

    let failure = if start_server && failure.as_ref().is_some_and(daemon_is_spawnable) {
        if let Some(error) = tmux_label_creation_error(socket_path, socket_source, start_server) {
            eprintln!("{}", error.message);
            let nested_label_new_session =
                canonical_command(&command) == "new-session" && error.kind == ErrorKind::NotFound;
            return Some(if nested_label_new_session {
                ExitCode::SUCCESS
            } else {
                exit_code_for(CliFailure::Runtime)
            });
        }
        let client = match spawn_and_connect_command_client(socket_path, mux_config_files) {
            Ok(connected) => connected,
            Err(error) => {
                eprintln!("{}", format_local_command_error(socket_path, error));
                return Some(exit_code_for(CliFailure::Runtime));
            }
        };
        let (client, spawned_server_id) = client;
        match run_local_command_chain(
            &routing,
            client,
            &command_chain,
            progress_target.as_deref(),
            Some(spawned_server_id),
        ) {
            LocalChain::Done(exit) => return Some(exit),
            LocalChain::Resume(prepared, client) => {
                resume = Some((prepared, *client));
                None
            }
            LocalChain::Unanswered(error) => {
                eprintln!("{}", format_local_command_error(socket_path, error));
                return Some(exit_code_for(CliFailure::Runtime));
            }
        }
    } else {
        failure
    };

    if command == "kill-server" && resume.is_none() {
        return Some(run_kill_server(socket_path, invocation.args));
    }

    let (new_session_tui, native_attach) = match &resume {
        Some((resume, _)) => (
            resume.kind == ExecResumeKind::NewSession,
            resume.kind == ExecResumeKind::NativeAttach,
        ),
        None => (
            command_chain_uses_tui(&command_chain) || attach_prefix_uses_tui(&command),
            matches!(command.as_str(), "attach" | "attach-session"),
        ),
    };
    let (prepared, connection) = match resume {
        Some((resume, client)) => (Some(resume.commands), Some(client)),
        None => (None, None),
    };
    if (prepared.is_some() || start_server) && new_session_tui {
        return Some(run_tui_chain(&routing, prepared, command_chain, connection));
    }
    if native_attach {
        return Some(run_native_attach(
            &routing,
            prepared,
            command_chain,
            connection,
        ));
    }

    if let Some(error) = tmux_label_creation_error(socket_path, socket_source, start_server) {
        eprintln!("{}", error.message);
        let nested_label_new_session =
            canonical_command(&command) == "new-session" && error.kind == ErrorKind::NotFound;
        return Some(if nested_label_new_session {
            ExitCode::SUCCESS
        } else {
            exit_code_for(CliFailure::Runtime)
        });
    }
    let failure = failure.unwrap_or_else(|| {
        DaemonError::Server(ServerError::Internal(
            "daemon handed back a chain it cannot resume".to_owned(),
        ))
    });
    eprintln!("{}", format_local_command_error(socket_path, failure));
    Some(exit_code_for(CliFailure::Runtime))
}

struct ChainRouting<'a> {
    socket_path: &'a Path,
    host: Option<&'a str>,
    mux_config_files: &'a [PathBuf],
    startup_options: &'a StartupOptions,
    command: &'a str,
    start_server: bool,
}

enum LocalChain {
    Done(ExitCode),
    Resume(ExecResume, Box<CommandClient>),
    Unanswered(DaemonError),
}

fn prepare_command_client(client: &mut CommandClient) {
    client.enable_stdin();
    client.set_stderr_handler(print_command_error);
    client.set_stdout_handler(print_released_command_output);
}

fn emit_chain_outcome(outcome: &CommandOutcome) -> u8 {
    let status = print_chain_output(&outcome.stdout, raw_command_output(outcome.stdout_claim));
    print_command_error(&outcome.stderr);
    status
}

fn run_local_command_chain(
    routing: &ChainRouting<'_>,
    mut client: CommandClient,
    command_chain: &[CommandInvocation],
    progress_target: Option<&str>,
    spawned_server_id: Option<u64>,
) -> LocalChain {
    let _progress = if command_chain.len() == 1
        && let Some(pane) = progress_target
    {
        match InteractiveClient::connect_control(routing.socket_path)
            .map_err(|error| format_local_command_error(routing.socket_path, error))
            .and_then(|client| events::Progress::start(client, pane.to_owned()))
        {
            Ok(progress) => Some(progress),
            Err(error) => {
                eprintln!("agent-send: --progress: {error}");
                return LocalChain::Done(exit_code_for(CliFailure::Runtime));
            }
        }
    } else {
        None
    };
    prepare_command_client(&mut client);
    let classify: ExecClassifier<'_> = &zz_daemon::exec_resume_kind;
    let chain = ExecChain {
        commands: command_chain.to_vec(),
        spawned_server_id,
        expect_server_id: None,
        resume: Some(classify),
        prepared: false,
        last: true,
    };
    match client.exec_chain(chain, emit_chain_outcome) {
        Ok(ExecChainEnd::Ran { exit_code }) => LocalChain::Done(ExitCode::from(exit_code)),
        Ok(ExecChainEnd::Resume(resume)) => LocalChain::Resume(resume, Box::new(client)),
        Ok(ExecChainEnd::Rejected(error)) => {
            eprintln!("{}", server_error_message(&error));
            LocalChain::Done(exit_code_for(CliFailure::Server(&error)))
        }
        Ok(ExecChainEnd::Failed(error))
            if routing.command == "kill-server" && daemon_transport_failure(&error) =>
        {
            LocalChain::Done(recover_kill_server_failure(routing.socket_path, &error))
        }
        Ok(ExecChainEnd::Failed(error)) => LocalChain::Done(print_chain_failure(error, None)),
        Ok(ExecChainEnd::ServerMismatch) => LocalChain::Done(exit_code_for(CliFailure::Runtime)),
        Err(error) => LocalChain::Unanswered(error),
    }
}

fn print_chain_failure(error: DaemonError, host: Option<&str>) -> ExitCode {
    match error {
        DaemonError::CommandFailed { output, error } => {
            print_chain_output(&output, false);
            eprintln!("{}", with_host_hint(host, command_error_message(&error)));
            exit_code_for(CliFailure::Runtime)
        }
        error => {
            eprintln!("{}", with_host_hint(host, command_error_message(&error)));
            exit_code_for(CliFailure::Daemon(&error))
        }
    }
}

fn relay_declined(error: &DaemonError) -> bool {
    match error {
        DaemonError::Server(error) => error.is_command_parse(),
        DaemonError::CommandFailed { error, .. } => relay_declined(error),
        _ => false,
    }
}

fn relay_agent_send(
    socket_path: &Path,
    host: &str,
    command_chain: &mut [CommandInvocation],
) -> Option<ExitCode> {
    let [invocation] = command_chain else {
        return None;
    };
    if canonical_command(&invocation.name) != "agent-send" || std::env::var_os("ZZ_PANE").is_none()
    {
        return None;
    }
    if zz_daemon::command_stdin_sink("agent-send", &invocation.args).is_some() {
        if io::stdin().is_terminal() {
            return None;
        }
        let mut piped = Vec::new();
        io::Read::read_to_end(&mut io::stdin().lock(), &mut piped).ok()?;
        zz_daemon::append_stdin_payload(
            "agent-send",
            &mut invocation.args,
            String::from_utf8_lossy(&piped).into_owned(),
        );
    }
    let arguments = ["--relay", host]
        .into_iter()
        .map(RawText::from)
        .chain(invocation.args.iter().cloned());
    let mut client = CommandClient::connect(socket_path).ok()?;
    match client.execute_streams(CommandInvocation::new("agent-send", arguments)) {
        Ok(outcome) => {
            print_command_output(&outcome.stdout);
            print_command_error(&outcome.stderr);
            Some(ExitCode::from(outcome.exit_code))
        }
        Err(error) if relay_declined(&error) => None,
        Err(error) => {
            eprintln!("{}", command_error_message(&error));
            Some(exit_code_for(CliFailure::Daemon(&error)))
        }
    }
}

fn run_host_command_chain(
    routing: &ChainRouting<'_>,
    host: &str,
    socket_source: SocketSelectionSource,
    mut command_chain: Vec<CommandInvocation>,
    progress_target: Option<&str>,
) -> ExitCode {
    if command_chain_uses_tui(&command_chain) || attach_prefix_uses_tui(routing.command) {
        return run_tui_chain(routing, None, command_chain, None);
    }
    if matches!(routing.command, "attach" | "attach-session") {
        return run_native_attach(routing, None, command_chain, None);
    }
    if let Some(exit) = relay_agent_send(routing.socket_path, host, &mut command_chain) {
        return exit;
    }
    if let Some(error) =
        tmux_label_creation_error(routing.socket_path, socket_source, routing.start_server)
    {
        eprintln!("{}", error.message);
        let nested_label_new_session = canonical_command(routing.command) == "new-session"
            && error.kind == ErrorKind::NotFound;
        return if nested_label_new_session {
            ExitCode::SUCCESS
        } else {
            exit_code_for(CliFailure::Runtime)
        };
    }
    let mut client = match connect_host_command_client(host, chain_is_read_only(&command_chain)) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("zz: {error}");
            return exit_code_for(CliFailure::Runtime);
        }
    };
    let _progress = if command_chain.len() == 1
        && let Some(pane) = progress_target
    {
        match configured_host_endpoint(host)
            .and_then(|endpoint| {
                InteractiveClient::connect_control_endpoint(&endpoint)
                    .map_err(|error| host_command_error_message(host, &error))
            })
            .and_then(|client| events::Progress::start(client, pane.to_owned()))
        {
            Ok(progress) => Some(progress),
            Err(error) => {
                eprintln!("agent-send: --progress: {error}");
                return exit_code_for(CliFailure::Runtime);
            }
        }
    } else {
        None
    };
    prepare_command_client(&mut client);
    COMMAND_HOST.set(Some(host.to_owned()));
    match client.exec_chain(ExecChain::new(command_chain), emit_chain_outcome) {
        Ok(ExecChainEnd::Ran { exit_code }) => ExitCode::from(exit_code),
        Ok(ExecChainEnd::Rejected(error)) => {
            eprintln!(
                "{}",
                with_host_hint(Some(host), server_error_message(&error))
            );
            exit_code_for(CliFailure::Server(&error))
        }
        Ok(ExecChainEnd::Failed(error)) => print_chain_failure(error, Some(host)),
        Ok(ExecChainEnd::Resume(_) | ExecChainEnd::ServerMismatch) => {
            exit_code_for(CliFailure::Runtime)
        }
        Err(error) => {
            eprintln!("{}", host_command_error_message(host, &error));
            exit_code_for(CliFailure::Daemon(&error))
        }
    }
}

fn tui_request<'a>(
    routing: &ChainRouting<'a>,
    options: &'a zz_tui::RunOptions,
    reconnect: &'a dyn Fn(
        &Path,
        bool,
        Option<zz_protocol::AttachOperation>,
    ) -> Result<InteractiveClient, DaemonError>,
) -> zz_tui::RunRequest<'a> {
    match routing.startup_options.browser_provider {
        Some(provider) => options.with_browser_provider(provider),
        None => zz_tui::RunRequest::from(options),
    }
    .with_local_reconnect(reconnect)
}

fn run_tui_chain(
    routing: &ChainRouting<'_>,
    prepared: Option<Vec<PreparedCommand>>,
    command_chain: Vec<CommandInvocation>,
    connection: Option<CommandClient>,
) -> ExitCode {
    let options = zz_tui::RunOptions {
        socket_path: routing.socket_path.to_path_buf(),
        host: routing.host.map(str::to_owned),
        session: None,
        restart_daemon: false,
        detach_others: false,
        read_only: false,
        client_flags: None,
    };
    let mux_config_files = routing.mux_config_files;
    let reconnect =
        |path: &Path, client_has_terminal, attach: Option<zz_protocol::AttachOperation>| {
            connect_terminal_surface_client_with_config(
                path,
                TerminalColorScheme::Dark,
                mux_config_files,
                client_has_terminal,
                attach.as_ref(),
            )
        };
    let request = tui_request(routing, &options, &reconnect);
    let result = match (connection, prepared) {
        (Some(client), Some(commands)) => {
            match client.into_interactive(zz_protocol::AttachOperation::Commands(commands.clone()))
            {
                Ok(initial) => zz_tui::run_connected(request, initial),
                Err(DaemonError::DaemonTooOld { .. }) => {
                    zz_tui::run_prepared_new_session(request, commands)
                }
                Err(error) => {
                    eprintln!("{}", command_error_message(&error));
                    return exit_code_for(CliFailure::Daemon(&error));
                }
            }
        }
        (_, Some(commands)) => zz_tui::run_prepared_new_session(request, commands),
        (_, None) => zz_tui::run_new_session(request, command_chain),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", tui_failure_message(routing.host, &error));
            exit_code_for(CliFailure::Runtime)
        }
    }
}

#[cfg(test)]
fn prepared_command_invocations(command: &PreparedCommand) -> Option<Cow<'_, [CommandInvocation]>> {
    if command.result != zz_protocol::PreparedCommandResult::Ready {
        return None;
    }
    if command.canonical_name.is_some() {
        return Some(Cow::Borrowed(std::slice::from_ref(&command.invocation)));
    }
    MuxEngine::command_alias_group_commands(&command.invocation)
        .ok()
        .flatten()
        .map(Cow::Owned)
}

#[cfg(test)]
fn prepared_command_reads_stdin(command: &PreparedCommand) -> Option<CommandStdinSink> {
    prepared_command_invocations(command)?
        .iter()
        .find_map(|invocation| {
            let canonical_name = command
                .canonical_name
                .as_deref()
                .unwrap_or_else(|| canonical_command(&invocation.name));
            zz_daemon::command_stdin_sink(canonical_name, &invocation.args)
        })
}

#[cfg(test)]
fn append_prepared_command_stdin_payload(
    command: &mut PreparedCommand,
    payload: impl Into<RawText>,
) {
    if let Some(canonical_name) = command.canonical_name.clone() {
        append_stdin_payload(&canonical_name, &mut command.invocation.args, payload);
        return;
    }
    command.invocation.set_stdin(payload);
}

fn new_session_uses_tui(invocation: &CommandInvocation) -> bool {
    canonical_command(&invocation.name) == "new-session"
        && MuxEngine::new_session_attaches(&invocation.args).unwrap_or(false)
}

fn attach_prefix_uses_tui(command: &str) -> bool {
    canonical_command(command) == "attach-session"
        && !matches!(command, "attach" | "attach-session")
}

#[cfg(test)]
fn command_reads_stdin(invocation: &CommandInvocation) -> Option<CommandStdinSink> {
    zz_daemon::command_stdin_sink(canonical_command(&invocation.name), &invocation.args)
}

fn command_chain_uses_tui(invocations: &[CommandInvocation]) -> bool {
    invocations.iter().any(new_session_uses_tui)
}

fn split_command_chain(arguments: &[RawText]) -> Vec<CommandInvocation> {
    zz_protocol::split_command_words(arguments.iter().cloned())
        .into_iter()
        .filter_map(|words| {
            let mut words = words.into_iter();
            let name = words.next()?;
            Some(CommandInvocation::new(name, words))
        })
        .collect()
}

fn run_tmux_shell_command(
    socket_path: &Path,
    socket_source: SocketSelectionSource,
    host: Option<&str>,
    mux_config_files: &[PathBuf],
    no_start_server: bool,
    shell_command: &str,
    login_shell: bool,
) -> ExitCode {
    let start_server = !no_start_server;
    if let Some(error) = tmux_label_creation_error(socket_path, socket_source, start_server) {
        eprintln!("{}", error.message);
        return exit_code_for(CliFailure::Runtime);
    }
    let mut client = match host.map_or_else(
        || {
            connect_command_client(socket_path, mux_config_files, start_server)
                .map_err(|error| format_local_command_error(socket_path, error))
        },
        |host| connect_host_command_client(host, false).map_err(|error| format!("zz: {error}")),
    ) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return exit_code_for(CliFailure::Runtime);
        }
    };
    let shell = match client.execute(CommandInvocation::new(
        "show-options",
        ["-gqv", "default-shell"],
    )) {
        Ok(shell) => shell.trim_end_matches('\n').to_owned(),
        Err(error) => {
            eprintln!(
                "{}",
                host.map_or_else(
                    || command_error_message(&error),
                    |host| host_command_error_message(host, &error)
                )
            );
            return exit_code_for(CliFailure::Daemon(&error));
        }
    };
    let mut process = Command::new(&shell);
    #[cfg(unix)]
    {
        use std::{ffi::OsString, os::unix::process::CommandExt as _};

        let name = Path::new(&shell)
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new(&shell));
        let mut argv0 = OsString::new();
        if login_shell {
            argv0.push("-");
        }
        argv0.push(name);
        process.arg0(argv0);
    }
    #[cfg(not(unix))]
    let _ = login_shell;
    match process
        .arg("-c")
        .arg(shell_command)
        .env("SHELL", &shell)
        .status_unmasked()
    {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(exit_code_for(CliFailure::Runtime), ExitCode::from),
        Err(error) => {
            eprintln!("zz: could not run {shell}: {error}");
            exit_code_for(CliFailure::Runtime)
        }
    }
}

fn is_version_command(command: &str) -> bool {
    command == "--version"
}

fn tmux_command_starts_server(command: &str) -> bool {
    matches!(
        canonical_command(command),
        "attach-session" | "list-commands" | "list-keys" | "new-session" | "start-server"
    )
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
