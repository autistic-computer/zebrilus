use std::fmt;

use semver::Version;
use zz_protocol::{MIN_CLIENT_REVISION, MIN_DAEMON_REVISION, PROTOCOL_VERSION, REVISION};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildInfo {
    pub version: Version,
    pub epoch: u16,
    pub revision: u32,
    pub min_client_revision: u32,
    pub min_daemon_revision: u32,
}

impl BuildInfo {
    pub fn current() -> Self {
        Self {
            version: Version::parse(env!("CARGO_PKG_VERSION")).expect("the zz version is semver"),
            epoch: PROTOCOL_VERSION,
            revision: REVISION,
            min_client_revision: MIN_CLIENT_REVISION,
            min_daemon_revision: MIN_DAEMON_REVISION,
        }
    }
}

impl fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "version={}\nepoch={}\nrevision={}\nmin-client-revision={}\nmin-daemon-revision={}",
            self.version,
            self.epoch,
            self.revision,
            self.min_client_revision,
            self.min_daemon_revision
        )
    }
}
