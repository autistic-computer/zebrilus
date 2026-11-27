use super::*;

pub(super) fn tmux_label_creation_error(
    path: &Path,
    socket_source: SocketSelectionSource,
    start_server: bool,
) -> Option<TmuxLabelCreationError> {
    if socket_source != SocketSelectionSource::Label || !start_server {
        return None;
    }
    let parent = path.parent()?;
    match std::fs::metadata(parent) {
        Ok(metadata) if metadata.is_dir() => None,
        Ok(_) => Some(TmuxLabelCreationError {
            message: format!("error connecting to {} (Not a directory)", path.display()),
            kind: ErrorKind::NotADirectory,
        }),
        Err(error) => Some(TmuxLabelCreationError {
            message: format!(
                "error creating {} ({})",
                path.display(),
                os_error_text(&error)
            ),
            kind: error.kind(),
        }),
    }
}

fn os_error_text(error: &io::Error) -> String {
    let message = error.to_string();
    error.raw_os_error().map_or(message.clone(), |code| {
        message
            .strip_suffix(&format!(" (os error {code})"))
            .unwrap_or(&message)
            .to_owned()
    })
}

pub(super) fn protocol_version_output(
    mut args: impl Iterator<Item = String>,
    socket_overridden: bool,
) -> Result<String, &'static str> {
    if socket_overridden || args.next().is_some() {
        return Err("usage: zz protocol-version");
    }
    Ok(PROTOCOL_VERSION.to_string())
}

pub(super) fn format_local_daemon_error(error: DaemonError) -> String {
    match error {
        error if zz_daemon_client::HostFailure::classify(&error).client_too_old() => {
            error.to_string()
        }
        error @ (DaemonError::IncompatibleDaemon { .. } | DaemonError::DaemonTooOld { .. }) => {
            format!("{error}\nrun 'zz kill-server' to restart it (sessions will be lost)")
        }
        error => error.to_string(),
    }
}

pub(super) fn format_local_command_error(path: &Path, error: DaemonError) -> String {
    match error {
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "server exited unexpectedly" =>
        {
            message
        }
        DaemonError::Io(error) if error.kind() == ErrorKind::ConnectionRefused => {
            format!("no server running on {}", path.display())
        }
        #[cfg(unix)]
        DaemonError::Io(error) if error.raw_os_error() == Some(libc::EPERM) => format!(
            "error connecting to {} ({}): a sandbox keeps this command away from the zz daemon; rerun it with escalated permissions",
            path.display(),
            os_error_text(&error)
        ),
        DaemonError::Io(error) => format!(
            "error connecting to {} ({})",
            path.display(),
            os_error_text(&error)
        ),
        error => format!("zz: {}", format_local_daemon_error(error)),
    }
}

pub(super) const fn raw_command_output(claim: StdoutClaim) -> bool {
    matches!(claim, StdoutClaim::Raw)
}

#[derive(Default)]
pub(super) struct CommandOutputWriter {
    raw_owner: Option<bool>,
}

impl CommandOutputWriter {
    #[cfg(unix)]
    pub(super) const OWNED_STREAM_ERROR: i32 = libc::EBADF;
    #[cfg(windows)]
    pub(super) const OWNED_STREAM_ERROR: i32 = 6;

    pub(super) fn write(
        &mut self,
        output: &RawText,
        raw: bool,
        stdout: &mut impl io::Write,
    ) -> io::Result<()> {
        let output = output.as_bytes();
        if output.is_empty() {
            return Ok(());
        }
        if raw && self.raw_owner.is_some() {
            return Err(io::Error::from_raw_os_error(Self::OWNED_STREAM_ERROR));
        }
        if self.raw_owner == Some(true) {
            return Ok(());
        }
        self.raw_owner = Some(raw);
        stdout.write_all(output)?;
        if !raw && !output.ends_with(b"\n") {
            stdout.write_all(b"\n")?;
        }
        stdout.flush()
    }

    fn print(&mut self, output: &RawText, raw: bool) -> u8 {
        if let Err(error) = self.write(output, raw, &mut io::stdout().lock())
            && raw
            && error.raw_os_error() == Some(Self::OWNED_STREAM_ERROR)
        {
            eprintln!("{}: -", os_error_text(&error));
            return 1;
        }
        0
    }
}

thread_local! {
    static CHAIN_OUTPUT_WRITER: RefCell<CommandOutputWriter> =
        const { RefCell::new(CommandOutputWriter { raw_owner: None }) };
}

pub(super) fn print_chain_output(output: &RawText, raw: bool) -> u8 {
    CHAIN_OUTPUT_WRITER.with(|writer| writer.borrow_mut().print(output, raw))
}

pub(super) fn print_released_command_output(output: &RawText) {
    let _ = print_chain_output(output, false);
}

pub(super) fn print_command_output(output: &RawText) {
    CommandOutputWriter::default().print(output, false);
}

pub(super) fn print_command_error(output: &str) {
    if output.is_empty() {
        return;
    }
    let hosted = COMMAND_HOST
        .with_borrow(|host| host.as_deref().map(|host| with_host_commands(host, output)));
    let output = hosted.as_deref().unwrap_or(output);
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(output.as_bytes());
    if !output.ends_with('\n') {
        let _ = stderr.write_all(b"\n");
    }
    let _ = stderr.flush();
}

#[derive(Clone, Copy)]
pub(super) enum CliFailure<'a> {
    Usage,
    TmuxUsage,
    Runtime,
    Server(&'a ServerError),
    Daemon(&'a DaemonError),
}

pub(super) fn exit_code_for(error: CliFailure<'_>) -> ExitCode {
    ExitCode::from(match error {
        CliFailure::Usage => 2,
        CliFailure::Runtime | CliFailure::TmuxUsage => 1,
        CliFailure::Server(error) => error.exit_code(),
        CliFailure::Daemon(error) => match error {
            DaemonError::Server(error) => error.exit_code(),
            DaemonError::CommandExit { exit_code, .. }
            | DaemonError::ReportedCommandExit { exit_code, .. } => *exit_code,
            _ => 1,
        },
    })
}

pub(super) fn command_error_message(error: &DaemonError) -> String {
    match error {
        DaemonError::CommandFailed { error, .. } => command_error_message(error),
        DaemonError::Server(error) => server_error_message(error),
        error => format!("zz: {error}"),
    }
}

pub(super) fn server_error_message(error: &ServerError) -> String {
    error.tmux_message()
}
