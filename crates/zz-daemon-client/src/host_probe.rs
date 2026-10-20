use crate::{
    SshEndpoint,
    endpoint::{
        SSH_FAILURE_STATUS, SshFailure, probe_value, remote_socket_probe_script, run_remote_script,
        ssh_failure_reason,
    },
};

const PROBE_CONNECT_TIMEOUT_SECONDS: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostStatus {
    Ready,
    Incompatible,
    Missing,
    Auth,
    Unreachable,
}

impl HostStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Incompatible => "incompatible",
            Self::Missing => "missing",
            Self::Auth => "auth",
            Self::Unreachable => "unreachable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostProbe {
    pub status: HostStatus,
    pub version: String,
    pub protocol: String,
    pub error: String,
}

impl HostProbe {
    #[must_use]
    pub fn failed(status: HostStatus, error: impl Into<String>) -> Self {
        Self {
            status,
            version: String::new(),
            protocol: String::new(),
            error: error.into(),
        }
    }
}

pub fn probe_host(endpoint: &SshEndpoint) -> HostProbe {
    match run_remote_script(
        endpoint,
        PROBE_CONNECT_TIMEOUT_SECONDS,
        &host_probe_script(),
    ) {
        Ok(output) => host_probe_from(
            output.status.code(),
            &output.stdout,
            &output.stderr,
            &output.status.to_string(),
        ),
        Err(error) => HostProbe::failed(
            HostStatus::Unreachable,
            format!("could not run ssh: {error}"),
        ),
    }
}

fn host_probe_script() -> String {
    format!(
        "{}; if command -v \"$zz_cli\" >/dev/null 2>&1; then \
         printf 'zz-probe-version=%s\\n' \"$(\"$zz_cli\" --version 2>/dev/null)\"; fi",
        remote_socket_probe_script(None),
    )
}

fn host_probe_from(code: Option<i32>, stdout: &[u8], stderr: &[u8], status: &str) -> HostProbe {
    let stderr = String::from_utf8_lossy(stderr);
    if code == Some(SSH_FAILURE_STATUS) {
        let status_of_host = if SshFailure::classify(&stderr).is_some_and(SshFailure::is_login) {
            HostStatus::Auth
        } else {
            HostStatus::Unreachable
        };
        return HostProbe::failed(status_of_host, ssh_failure_reason(&stderr, status));
    }
    let stdout = String::from_utf8_lossy(stdout);
    let version = probe_value(&stdout, "zz-probe-version=")
        .and_then(|version| version.split_whitespace().last())
        .unwrap_or_default()
        .to_owned();
    let name = zz_protocol::app_identity::DIRECTORY;
    let client = zz_protocol::PROTOCOL_VERSION;
    let (status_of_host, protocol, error) = match probe_value(&stdout, "zz-probe-protocol=") {
        Some("missing") => (HostStatus::Missing, "", format!("{name} is not installed")),
        Some("unknown") => (
            HostStatus::Incompatible,
            "",
            format!("{name} there predates protocol version reporting"),
        ),
        Some(protocol) if protocol == client.to_string() => {
            (HostStatus::Ready, protocol, String::new())
        }
        Some(protocol) if protocol.parse::<u16>().is_ok() => (
            HostStatus::Incompatible,
            protocol,
            format!("speaks protocol v{protocol}; this {name} speaks v{client}"),
        ),
        _ => (
            HostStatus::Unreachable,
            "",
            if stderr.trim().is_empty() {
                format!("the probe printed no protocol version ({status})")
            } else {
                ssh_failure_reason(&stderr, status)
            },
        ),
    };
    HostProbe {
        status: status_of_host,
        version,
        protocol: protocol.to_owned(),
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(code: i32, stdout: &str, stderr: &str) -> HostProbe {
        host_probe_from(
            Some(code),
            stdout.as_bytes(),
            stderr.as_bytes(),
            &format!("exit status: {code}"),
        )
    }

    #[test]
    fn a_probe_reads_as_one_of_five_states() {
        let ours = zz_protocol::PROTOCOL_VERSION;
        let ready = probe(
            0,
            &format!(
                "motd\nzz-probe-socket=/run/user/1000/zz/default.sock\nzz-probe-protocol={ours}\n\
                 zz-probe-version=zz 0.19.0\n"
            ),
            "",
        );
        assert_eq!(
            ready,
            HostProbe {
                status: HostStatus::Ready,
                version: "0.19.0".to_owned(),
                protocol: ours.to_string(),
                error: String::new(),
            }
        );

        let old = probe(
            0,
            "zz-probe-socket=/tmp/zz-a/default.sock\nzz-probe-protocol=107\nzz-probe-version=zz 0.18.0\n",
            "",
        );
        assert_eq!(
            (old.status, old.version.as_str(), old.protocol.as_str()),
            (HostStatus::Incompatible, "0.18.0", "107")
        );
        assert!(old.error.contains("v107"), "{}", old.error);
        assert_eq!(
            probe(
                0,
                "zz-probe-socket=/tmp/zz-a/default.sock\nzz-probe-protocol=unknown\n",
                ""
            )
            .status,
            HostStatus::Incompatible
        );

        let missing = probe(
            0,
            "zz-probe-socket=/tmp/zz-a/default.sock\nzz-probe-protocol=missing\n",
            "",
        );
        assert_eq!(
            (missing.status, missing.version.as_str()),
            (HostStatus::Missing, "")
        );

        for (stderr, status) in [
            (
                "demfabris@alienware: Permission denied (publickey,password).",
                HostStatus::Auth,
            ),
            ("Host key verification failed.", HostStatus::Auth),
            (
                "ssh: connect to host ubuntu port 22: Operation timed out",
                HostStatus::Unreachable,
            ),
            (
                "ssh: Could not resolve hostname gpu: nodename nor servname provided",
                HostStatus::Unreachable,
            ),
        ] {
            let failed = probe(255, "", stderr);
            assert_eq!(failed.status, status, "{stderr}");
            assert_eq!(failed.error, ssh_failure_reason(stderr, "exit status: 255"));
        }

        let broken = probe(1, "Welcome\n", "sh: syntax error\n");
        assert_eq!(
            (broken.status, broken.error.as_str()),
            (HostStatus::Unreachable, "sh: syntax error")
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_host_probe_reports_the_version_and_writes_nothing() {
        use std::os::unix::fs::PermissionsExt as _;

        let home = tempfile::tempdir().expect("temporary home");
        let bin = home.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let zz = bin.join(zz_protocol::app_identity::DIRECTORY);
        std::fs::write(
            &zz,
            format!(
                "#!/bin/sh\ncase \"$1\" in protocol-version) echo {};; --version) echo 'zz 9.9.9';; esac\n",
                zz_protocol::PROTOCOL_VERSION
            ),
        )
        .unwrap();
        std::fs::set_permissions(&zz, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                &format!("sh -c {}", crate::shell_quote(&host_probe_script())),
            ])
            .env("HOME", home.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .unwrap();
        let probe = host_probe_from(
            output.status.code(),
            &output.stdout,
            &output.stderr,
            "exit status: 0",
        );
        assert_eq!(
            (probe.status, probe.version.as_str()),
            (HostStatus::Ready, "9.9.9")
        );
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
    }
}
