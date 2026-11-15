use std::path::{Path, PathBuf};

#[cfg(unix)]
use crate::endpoint::connect_kept;
use crate::{
    DaemonError,
    client::tailnet_failure,
    endpoint::{Endpoint, SshForward, SshLogin},
    tailnet::TailnetForward,
};

pub struct DaemonRoute {
    socket: PathBuf,
    _ssh: Option<SshForward>,
    _tailnet: Option<TailnetForward>,
}

impl DaemonRoute {
    pub fn open_unattended(endpoint: &Endpoint) -> Result<Self, DaemonError> {
        match endpoint {
            Endpoint::Local(path) => Ok(Self {
                socket: path.clone(),
                _ssh: None,
                _tailnet: None,
            }),
            Endpoint::Tailnet(endpoint) => {
                let forward = TailnetForward::start(endpoint)
                    .map_err(|error| tailnet_failure(endpoint, &error))?;
                Ok(Self {
                    socket: forward.socket().to_owned(),
                    _ssh: None,
                    _tailnet: Some(forward),
                })
            }
            Endpoint::Ssh(endpoint) => {
                #[cfg(unix)]
                let (socket, forward) =
                    connect_kept(endpoint, SshLogin::Unattended, true, |socket, _| {
                        Ok(socket.to_owned())
                    })?;
                #[cfg(windows)]
                let (socket, forward) = {
                    let forward = SshForward::start(endpoint, SshLogin::Unattended)?;
                    (forward.local_socket().to_owned(), Some(forward))
                };
                Ok(Self {
                    socket,
                    _ssh: forward,
                    _tailnet: None,
                })
            }
        }
    }

    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }
}
