use std::io;

use zz_protocol::RawText;

use super::{CommandOutputWriter, command_error_message, protocol_version_output};
use zz_daemon_client::DaemonError;
use zz_protocol::ServerError;

#[test]
fn cli_exit_contract_preserves_explicit_status_and_classifies_errors() {
    use super::{CliFailure, exit_code_for};
    use std::process::ExitCode;

    for code in [0, 1, 2, 3, 124, 125, 255] {
        let error = DaemonError::CommandExit {
            output: RawText::default(),
            exit_code: code,
        };
        assert_eq!(
            exit_code_for(CliFailure::Daemon(&error)),
            ExitCode::from(code)
        );
    }
    let usage = ServerError::CommandParse("command list-panes: invalid flag --".to_owned());
    assert_eq!(exit_code_for(CliFailure::Server(&usage)), ExitCode::from(1));
    let unsupported = ServerError::UnsupportedCommand("unknown command: bogus".to_owned());
    assert_eq!(
        exit_code_for(CliFailure::Server(&unsupported)),
        ExitCode::from(1)
    );
    let native = ServerError::NativeCommandParse("native usage".to_owned());
    assert_eq!(
        exit_code_for(CliFailure::Server(&native)),
        ExitCode::from(2)
    );
    let runtime = ServerError::InvalidCommand("duplicate session: dup".to_owned());
    assert_eq!(
        exit_code_for(CliFailure::Server(&runtime)),
        ExitCode::from(1)
    );
    let missing = ServerError::InvalidTarget("can't find window: 9".to_owned());
    assert_eq!(
        exit_code_for(CliFailure::Server(&missing)),
        ExitCode::from(1)
    );
    let failed = DaemonError::CommandFailed {
        output: "partial output".into(),
        error: Box::new(DaemonError::Server(usage)),
    };
    assert_eq!(
        exit_code_for(CliFailure::Daemon(&failed)),
        ExitCode::from(1)
    );
    let disconnected = DaemonError::Io(io::Error::from(io::ErrorKind::BrokenPipe));
    assert_eq!(
        exit_code_for(CliFailure::Daemon(&disconnected)),
        ExitCode::from(1)
    );
}

#[test]
fn command_output_file_stream_owns_stdout_after_its_first_write() {
    let mut writer = CommandOutputWriter::default();
    let mut stdout = Vec::new();
    writer.write(&"hello".into(), true, &mut stdout).unwrap();
    writer.write(&"\n".into(), false, &mut stdout).unwrap();
    writer.write(&"AFTER\n".into(), false, &mut stdout).unwrap();
    assert_eq!(stdout, b"hello");
    assert_eq!(
        writer
            .write(&"again".into(), true, &mut stdout)
            .unwrap_err()
            .raw_os_error(),
        Some(CommandOutputWriter::OWNED_STREAM_ERROR)
    );
    assert_eq!(stdout, b"hello");
}

#[test]
fn command_output_print_stream_survives_a_later_file_collision() {
    let mut writer = CommandOutputWriter::default();
    let mut stdout = Vec::new();
    writer.write(&"\n".into(), false, &mut stdout).unwrap();
    assert_eq!(
        writer
            .write(&"hello".into(), true, &mut stdout)
            .unwrap_err()
            .raw_os_error(),
        Some(CommandOutputWriter::OWNED_STREAM_ERROR)
    );
    writer.write(&"AFTER\n".into(), false, &mut stdout).unwrap();
    assert_eq!(stdout, b"\nAFTER\n");
}

#[test]
fn command_output_empty_results_do_not_claim_stdout() {
    for raw in [false, true] {
        let mut writer = CommandOutputWriter::default();
        let mut stdout = Vec::new();
        writer.write(&RawText::default(), raw, &mut stdout).unwrap();
        writer.write(&"hello".into(), true, &mut stdout).unwrap();
        assert_eq!(stdout, b"hello");
    }
}

#[test]
fn protocol_version_command_prints_only_the_wire_version() {
    assert_eq!(
        protocol_version_output(std::iter::empty(), false).unwrap(),
        zz_protocol::PROTOCOL_VERSION.to_string()
    );
    assert_eq!(
        protocol_version_output(["extra".to_owned()].into_iter(), false),
        Err("usage: zz protocol-version")
    );
    assert_eq!(
        protocol_version_output(std::iter::empty(), true),
        Err("usage: zz protocol-version")
    );
}

#[test]
fn refresh_client_detached_error_has_no_zz_prefix() {
    assert_eq!(
        command_error_message(&DaemonError::Server(
            zz_protocol::ServerError::InvalidCommand("no current client".to_owned())
        )),
        "no current client"
    );
    assert_eq!(
        command_error_message(&DaemonError::Server(
            zz_protocol::ServerError::InvalidCommand("other".to_owned())
        )),
        "other"
    );
}

#[test]
fn a_newer_daemon_epoch_is_reported_without_a_restart_hint() {
    assert_eq!(
        super::format_local_daemon_error(DaemonError::IncompatibleDaemon {
            daemon: Some(111),
            client: 110,
        }),
        "this zz is too old for the running daemon: it speaks protocol v110 and the daemon speaks v111; update zz"
    );
}
