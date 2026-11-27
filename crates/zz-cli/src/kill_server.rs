use super::*;

pub(super) fn daemon_transport_failure(error: &DaemonError) -> bool {
    match error {
        DaemonError::Io(_) | DaemonError::Protocol(_) | DaemonError::IncompatibleDaemon { .. } => {
            true
        }
        DaemonError::CommandFailed { error, .. } => daemon_transport_failure(error),
        DaemonError::Server(_)
        | DaemonError::AlreadyRunning(_)
        | DaemonError::Thread(_)
        | DaemonError::InsertedCommandParse(_)
        | DaemonError::CommandExit { .. }
        | DaemonError::ReportedCommandExit { .. }
        | DaemonError::DaemonTooOld { .. }
        | DaemonError::DaemonLacksCapability { .. } => false,
    }
}

pub(super) fn run_kill_server(path: &Path, args: impl IntoIterator<Item = RawText>) -> ExitCode {
    let invocation = CommandInvocation::new("kill-server", args);
    let failure = match CommandClient::connect(path) {
        Ok(mut client) => match client.execute_prepared_streams(invocation) {
            Ok(outcome) => {
                print_command_output(&outcome.stdout);
                print_command_error(&outcome.stderr);
                return ExitCode::from(outcome.exit_code);
            }
            Err(error) => error,
        },
        Err(error) if daemon_is_missing(&error) => {
            eprintln!("{}", format_local_command_error(path, error));
            return exit_code_for(CliFailure::Runtime);
        }
        Err(error) => error,
    };

    if let DaemonError::Server(error) = &failure {
        eprintln!("{}", server_error_message(error));
        return exit_code_for(CliFailure::Server(error));
    }
    recover_kill_server_failure(path, &failure)
}

pub(super) fn run_host_kill_server(
    host: &str,
    args: impl IntoIterator<Item = RawText>,
) -> ExitCode {
    let mut client = match connect_host_command_client(host, false) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("zz: {error}");
            return exit_code_for(CliFailure::Runtime);
        }
    };
    match client.execute_prepared_streams(CommandInvocation::new("kill-server", args)) {
        Ok(outcome) => {
            print_command_output(&outcome.stdout);
            print_command_error(&outcome.stderr);
            ExitCode::from(outcome.exit_code)
        }
        Err(DaemonError::CommandFailed { output, error }) => {
            print_command_output(&output);
            eprintln!("{}", command_error_message(&error));
            exit_code_for(CliFailure::Runtime)
        }
        Err(error) => {
            eprintln!("{}", host_command_error_message(host, &error));
            exit_code_for(CliFailure::Daemon(&error))
        }
    }
}

pub(super) fn recover_kill_server_failure(path: &Path, failure: &DaemonError) -> ExitCode {
    log::warn!(
        target: "zz::diagnostics::process",
        "graceful kill-server failed path={} error={failure}; attempting verified recovery",
        path.display(),
    );
    match terminate_incompatible_daemon(path) {
        Ok(recovered) => {
            eprintln!("zz: terminated incompatible daemon pid {}", recovered.pid());
            ExitCode::SUCCESS
        }
        Err(recovery) => {
            eprintln!("zz: {failure}; recovery failed: {recovery}");
            exit_code_for(CliFailure::Runtime)
        }
    }
}

pub(super) fn daemon_is_missing(error: &DaemonError) -> bool {
    matches!(
        error,
        DaemonError::Io(error)
            if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused)
    )
}
