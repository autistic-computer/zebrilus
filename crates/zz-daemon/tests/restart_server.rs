#![cfg(unix)]

use std::{
    fs::{self, TryLockError},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use zz_daemon::{Daemon, Stopped};
use zz_daemon_client::CommandClient;
use zz_protocol::CommandInvocation;

fn connect(socket: &Path) -> CommandClient {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match CommandClient::connect(socket) {
            Ok(client) => return client,
            Err(error) if Instant::now() >= deadline => {
                panic!("daemon never accepted a client: {error}")
            }
            Err(_) => thread::sleep(Duration::from_millis(20)),
        }
    }
}

#[test]
fn restart_server_hands_over_the_start_lock_with_the_socket_gone() {
    let id = std::process::id();
    let socket = PathBuf::from(format!("/tmp/zz-restart-{id}.sock"));
    let lock = PathBuf::from(format!("/tmp/zz-restart-{id}.sock.lock"));
    let _ = fs::remove_file(&socket);
    let _ = fs::remove_file(&lock);

    let daemon = Daemon::new(&socket).without_user_config();
    let (sender, stopped) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(daemon.serve());
    });
    let mut client = connect(&socket);
    client
        .execute(CommandInvocation::new("new-session", ["-d", "-s", "kept"]))
        .expect("a session");
    let reply = client
        .execute(CommandInvocation::new("restart-server", [] as [&str; 0]))
        .expect("restart-server replies before the daemon stops");

    let restart = match stopped
        .recv_timeout(Duration::from_secs(10))
        .expect("the daemon stops")
    {
        Ok(Stopped::Restart(restart)) => restart,
        other => panic!("expected a restart, got {other:?}"),
    };
    assert_eq!(reply, "");
    assert!(
        !socket.exists(),
        "the socket is removed before the successor"
    );
    let contender = fs::OpenOptions::new()
        .write(true)
        .open(&lock)
        .expect("the start lock file stays for the successor");
    assert!(matches!(
        contender.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    assert!(restart.inheritable_lock_fd().is_some_and(|fd| fd > 2));

    drop(restart);
    assert!(!lock.exists());
    assert!(contender.try_lock().is_ok());
}
