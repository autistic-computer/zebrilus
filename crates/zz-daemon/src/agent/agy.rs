use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use agent_client_protocol::{AcpAgent, AcpAgentConfig};
use zz_protocol::{AGENT_ANTIGRAVITY_COMMAND, AgentProvider};

use crate::agent::{
    dialect::{self, Dialect},
    environment::agent_path,
    host::RuntimeChannels,
    runtime::AgentSpawnConfig,
};

const ACP: &str = "acp";
const ACP_USAGE: &str = "agy acp";
const USAGE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_USAGE_BYTES: usize = 64 * 1024;
const DIALECT: Dialect = Dialect {
    effort_option: "thought_level",
    signed_out: "Antigravity is signed out. Sign in by running `agy` in a terminal.",
};

pub(crate) async fn run_agy_runtime(
    config: AgentSpawnConfig,
    provider: AgentProvider,
    channels: RuntimeChannels,
) -> Result<(), String> {
    let inherited = std::env::var("PATH").ok();
    let agent = AcpAgent::new(
        AcpAgentConfig::new(program(agent_path().or(inherited.as_deref()))?).arg(ACP),
    );
    dialect::run(agent, DIALECT, config, provider, channels).await
}

fn program(path: Option<&str>) -> Result<PathBuf, String> {
    let installed = dialect::installed(path, AGENT_ANTIGRAVITY_COMMAND);
    let Some(first) = installed.first() else {
        return Err(
            "Antigravity is not installed: `agy` was not found on PATH. Install it from https://antigravity.google."
                .to_owned(),
        );
    };
    installed
        .iter()
        .find(|candidate| speaks_acp(candidate))
        .cloned()
        .ok_or_else(|| {
            format!(
                "Antigravity at {} is too old for chats: it has no `agy acp`. Update it with `agy update`.",
                first.display()
            )
        })
}

fn speaks_acp(program: &Path) -> bool {
    let mut command = Command::new(program);
    command.args([ACP, "--help"]).stdin(Stdio::null());
    crate::bounded_command::run_output_until(
        command,
        MAX_USAGE_BYTES,
        Instant::now() + USAGE_TIMEOUT,
    )
    .is_ok_and(|output| {
        [output.stdout, output.stderr]
            .iter()
            .any(|text| String::from_utf8_lossy(text).contains(ACP_USAGE))
    })
}
