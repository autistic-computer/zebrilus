use super::*;

pub(super) fn default_client_command_chain(
    socket_path: &Path,
    mux_config_files: &[PathBuf],
    no_start_server: bool,
) -> Vec<CommandInvocation> {
    stored_default_client_command(socket_path, mux_config_files, no_start_server)
        .map(|stored| zz_mux::parse_config("default-client-command", &stored))
        .filter(|parsed| parsed.diagnostics.is_empty() && !parsed.commands.is_empty())
        .map(|parsed| parsed.commands)
        .filter(|commands| !is_launcher_default_client_command(commands))
        .unwrap_or_else(|| vec![CommandInvocation::new("new-session", ["-A"])])
}

pub(super) fn bare_command_line_opens_application(
    origin: CommandLineOrigin,
    launched_by_launch_services: bool,
) -> bool {
    origin == CommandLineOrigin::Application && (cfg!(windows) || launched_by_launch_services)
}

#[cfg(target_os = "macos")]
pub(super) fn launched_by_launch_services() -> bool {
    std::os::unix::process::parent_id() == 1
}

#[cfg(not(target_os = "macos"))]
pub(super) fn launched_by_launch_services() -> bool {
    false
}

pub(super) fn is_launcher_default_client_command(commands: &[CommandInvocation]) -> bool {
    matches!(commands, [command] if command.name == "new-session" && command.args.is_empty())
}

fn stored_default_client_command(
    socket_path: &Path,
    mux_config_files: &[PathBuf],
    no_start_server: bool,
) -> Option<String> {
    let mut client =
        connect_command_client(socket_path, mux_config_files, !no_start_server).ok()?;
    let output = client
        .execute(CommandInvocation::new(
            "show-options",
            ["-sqv", "default-client-command"],
        ))
        .ok()?;
    Some(output.trim_end_matches('\n').to_owned())
}

fn startup_directory() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .filter(|directory| directory.is_dir())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|home| home.is_dir())
        })
}

#[cfg(target_os = "macos")]
pub(super) fn startup_directory_environment(directory: &Path) -> std::ffi::OsString {
    let mut environment = std::ffi::OsString::from(APP_STARTUP_DIRECTORY_ENV);
    environment.push("=");
    environment.push(directory);
    environment
}

#[cfg(target_os = "macos")]
#[must_use]
pub fn launch_application(socket_path: &Path) -> ExitCode {
    let launcher = match std::env::current_exe().and_then(|executable| executable.canonicalize()) {
        Ok(executable) => executable,
        Err(error) => {
            eprintln!("zz: could not resolve the launcher path: {error}");
            return ExitCode::FAILURE;
        }
    };

    let Some(bundle) = launcher
        .ancestors()
        .find(|path| path.extension() == Some(std::ffi::OsStr::new("app")))
    else {
        eprintln!(
            "zz app: the desktop app is not installed beside {}; this is a headless install",
            launcher.display(),
        );
        return ExitCode::FAILURE;
    };
    let mut command = Command::new("/usr/bin/open");
    command.args(["--env", "TMUX=", "--env", "TMUX_PANE="]);
    if let Some(directory) = startup_directory() {
        command.args([
            std::ffi::OsString::from("--env"),
            startup_directory_environment(&directory),
        ]);
    }
    if let Some(socket) = std::env::var_os("ZZ_SOCKET").filter(|socket| !socket.is_empty()) {
        let mut socket_environment = std::ffi::OsString::from("ZZ_SOCKET=");
        socket_environment.push(socket);
        command.args([std::ffi::OsString::from("--env"), socket_environment]);
    }
    command
        .arg(bundle)
        .args(["--args", "app", diagnostics::SOCKET_ARGUMENT])
        .arg(socket_path)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE");
    diagnostics::configure_spawned_process(&mut command);
    match command.status_unmasked() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("zz: could not open {}: {status}", bundle.display());
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("zz: could not open {}: {error}", bundle.display());
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[must_use]
pub fn launch_application(socket_path: &Path) -> ExitCode {
    let launcher = match std::env::current_exe().and_then(|executable| executable.canonicalize()) {
        Ok(executable) => executable,
        Err(error) => {
            eprintln!("zz: could not resolve the launcher path: {error}");
            return ExitCode::FAILURE;
        }
    };

    let roots = if cfg!(windows) {
        Vec::new()
    } else {
        let directory = zz_protocol::app_identity::DIRECTORY;
        std::iter::once(Path::new("/usr/lib").join(directory))
            .chain(
                zz_daemon_client::home_directory()
                    .map(|home| home.join(".local/lib").join(directory)),
            )
            .collect()
    };
    let Some(executable) = desktop_executable_from(&launcher, &roots) else {
        eprintln!(
            "zz app: the desktop app is not installed beside {}; this is a headless install",
            launcher.display(),
        );
        return ExitCode::FAILURE;
    };

    let mut command = Command::new(executable);
    command
        .args(["app", diagnostics::SOCKET_ARGUMENT])
        .arg(socket_path)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(directory) = startup_directory() {
        command.env(APP_STARTUP_DIRECTORY_ENV, directory);
    }
    diagnostics::configure_spawned_process(&mut command);
    match command.spawn_unmasked() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zz: could not open the application: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(any(not(target_os = "macos"), test))]
pub(super) fn desktop_executable_from(launcher: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    let name = if cfg!(windows) { "zz.exe" } else { "zz" };
    std::iter::once(launcher.with_file_name(name))
        .chain(roots.iter().map(|root| root.join(name)))
        .find(|candidate| {
            candidate.is_file() && candidate.canonicalize().ok().as_deref() != Some(launcher)
        })
}
