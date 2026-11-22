use std::path::{Path, PathBuf};

#[cfg(unix)]
use crate::endpoint::connect_kept;
use crate::{
    DaemonError, SshPrompts,
    client::tailnet_failure,
    endpoint::{Endpoint, SshForward, SshLogin},
    tailnet::TailnetForward,
};

pub struct DaemonRoute {
    socket: PathBuf,
    _ssh: Option<SshForward>,
    _tailnet: Option<TailnetForward>,
    #[cfg(windows)]
    _wsl: Option<crate::wsl::WslForward>,
}

impl DaemonRoute {
    pub fn open_unattended(endpoint: &Endpoint) -> Result<Self, DaemonError> {
        Self::open(endpoint, SshLogin::Unattended)
    }

    pub fn open_with_prompts(
        endpoint: &Endpoint,
        prompts: SshPrompts,
    ) -> Result<Self, DaemonError> {
        Self::open(endpoint, SshLogin::Prompts(Some(prompts)))
    }

    fn open(endpoint: &Endpoint, login: SshLogin) -> Result<Self, DaemonError> {
        match endpoint {
            Endpoint::Local(path) => Ok(Self {
                socket: path.clone(),
                _ssh: None,
                _tailnet: None,
                #[cfg(windows)]
                _wsl: None,
            }),
            Endpoint::Tailnet(endpoint) => {
                let forward = TailnetForward::start(endpoint)
                    .map_err(|error| tailnet_failure(endpoint, &error))?;
                Ok(Self {
                    socket: forward.socket().to_owned(),
                    _ssh: None,
                    _tailnet: Some(forward),
                    #[cfg(windows)]
                    _wsl: None,
                })
            }
            Endpoint::Wsl(endpoint) => {
                #[cfg(windows)]
                {
                    let forward = crate::wsl::WslForward::start(endpoint)?;
                    Ok(Self {
                        socket: forward.local_socket().to_owned(),
                        _ssh: None,
                        _tailnet: None,
                        _wsl: Some(forward),
                    })
                }
                #[cfg(not(windows))]
                {
                    let _ = endpoint;
                    Err(crate::EndpointError::UnsupportedPlatform.into())
                }
            }
            Endpoint::Ssh(endpoint) => {
                #[cfg(unix)]
                let (socket, forward) =
                    connect_kept(endpoint, login, true, |socket, _| Ok(socket.to_owned()))?;
                #[cfg(windows)]
                let (socket, forward) = {
                    let forward = SshForward::start(endpoint, login)?;
                    (forward.local_socket().to_owned(), Some(forward))
                };
                Ok(Self {
                    socket,
                    _ssh: forward,
                    _tailnet: None,
                    #[cfg(windows)]
                    _wsl: None,
                })
            }
        }
    }

    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }
}
