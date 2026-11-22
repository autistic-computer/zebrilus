use std::{path::PathBuf, time::Duration};

use zz_daemon_client::InteractiveClient;
use zz_protocol::PaneId;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = std::env::var_os("ZZ_SOCKET")
        .map(PathBuf::from)
        .ok_or("ZZ_SOCKET names the daemon to stop a turn on")?;
    let pane = std::env::args().nth(1).ok_or("usage: agent_cancel %PANE")?;
    let pane = PaneId(pane.trim_start_matches('%').parse()?);
    let client = InteractiveClient::connect_control(&socket)?;
    client.agent_cancel(pane)?;
    std::thread::sleep(Duration::from_millis(300));
    Ok(())
}
