use std::io;

use zz_protocol::RawText;

use super::{daemon_is_missing, daemon_transport_failure};
use zz_daemon_client::DaemonError;
use zz_protocol::ServerError;

#[test]
fn kill_recovery_accepts_only_transport_and_handshake_failures() {
    assert!(daemon_transport_failure(&DaemonError::Io(io::Error::from(
        io::ErrorKind::BrokenPipe
    ))));
    assert!(!daemon_transport_failure(&DaemonError::Server(
        ServerError::InvalidCommand("no".to_owned())
    )));
    assert!(!daemon_transport_failure(&DaemonError::CommandExit {
        output: RawText::default(),
        exit_code: 7,
    }));
    assert!(daemon_transport_failure(&DaemonError::CommandFailed {
        output: RawText::default(),
        error: Box::new(DaemonError::IncompatibleDaemon {
            daemon: Some(73),
            client: 74,
        }),
    }));
}

#[test]
fn kill_server_only_classifies_absent_endpoints_as_missing() {
    assert!(daemon_is_missing(&DaemonError::Io(io::Error::from(
        io::ErrorKind::NotFound
    ))));
    assert!(daemon_is_missing(&DaemonError::Io(io::Error::from(
        io::ErrorKind::ConnectionRefused
    ))));
    assert!(!daemon_is_missing(&DaemonError::Io(io::Error::from(
        io::ErrorKind::ConnectionReset
    ))));
}
