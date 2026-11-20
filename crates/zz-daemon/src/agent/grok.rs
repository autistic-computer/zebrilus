use std::path::PathBuf;

use agent_client_protocol::{AcpAgent, AcpAgentConfig};
use zz_protocol::{AGENT_GROK_COMMAND, AgentProvider};

use crate::agent::{
    cli_session,
    dialect::{self, Dialect},
    environment::agent_path,
    host::RuntimeChannels,
    runtime::AgentSpawnConfig,
};

const DIALECT: Dialect = Dialect {
    effort_option: "reasoning_effort",
    signed_out: "Grok is signed out. Sign in by running `grok login` in a terminal.",
    stored_title,
    sign_in: None,
};

pub(crate) async fn run_grok_runtime(
    config: AgentSpawnConfig,
    provider: AgentProvider,
    channels: RuntimeChannels,
) -> Result<(), String> {
    let inherited = std::env::var("PATH").ok();
    let agent = AcpAgent::new(
        AcpAgentConfig::new(program(agent_path().or(inherited.as_deref()))?)
            .arg("agent")
            .arg("stdio"),
    );
    dialect::run(agent, DIALECT, config, provider, channels).await
}

pub(crate) fn home() -> Option<PathBuf> {
    std::env::var_os("GROK_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".grok")))
}

fn stored_title(session: &str) -> Option<String> {
    let summary = cli_session::grok_summary(&home()?, session)?;
    ["generated_title", "session_summary"]
        .into_iter()
        .filter_map(|key| summary[key].as_str())
        .map(str::trim)
        .find(|title| !title.is_empty())
        .map(str::to_owned)
}

fn program(path: Option<&str>) -> Result<PathBuf, String> {
    dialect::installed(path, AGENT_GROK_COMMAND)
        .into_iter()
        .next()
        .ok_or_else(|| {
            "Grok is not installed: `grok` was not found on PATH. Install it with the script at https://x.ai/cli/install.sh."
                .to_owned()
        })
}
