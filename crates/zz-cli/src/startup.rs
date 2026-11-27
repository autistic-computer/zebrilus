use super::*;

pub(super) const TMUX_VERSION_OUTPUT: &str = zz_protocol::CommandSpec::TMUX_VERSION_OUTPUT;

pub(super) const TMUX_USAGE: &str = concat!(
    "usage: zz [-2CDhlNuVv] [-c shell-command] [-f file] [-L socket-name]\n",
    "            [-S socket-path] [-T features] [command [flags]]"
);

static HANDED_OVER_FROM: std::sync::OnceLock<String> = std::sync::OnceLock::new();

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

pub(super) fn application_working_directory(
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
pub(super) const MI_OPTION_PURGE_DELAY: std::ffi::c_int = 15;

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
pub(super) fn purge_freed_memory_promptly() {
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

enum SocketSelection {
    Path(PathBuf),
    Label(String),
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

pub(super) fn implicit_tmux_endpoint_conflict(
    socket_source: SocketSelectionSource,
    zz_socket: Option<&std::ffi::OsStr>,
    tmux: Option<&std::ffi::OsStr>,
) -> bool {
    socket_source == SocketSelectionSource::Default
        && zz_socket.is_none_or(std::ffi::OsStr::is_empty)
        && tmux.is_some_and(|value| !value.is_empty())
}

#[cfg(unix)]
pub(super) fn tmux_label_socket_path(
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
pub(super) fn tmux_socket_root(tmux_tmpdir: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    tmux_tmpdir
        .filter(|path| !path.is_empty())
        .and_then(|path| std::fs::canonicalize(Path::new(path)).ok())
        .or_else(|| std::fs::canonicalize("/tmp").ok())
}

#[cfg(not(unix))]
pub(super) fn tmux_label_socket_path(
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
