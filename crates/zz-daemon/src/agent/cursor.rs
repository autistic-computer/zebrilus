use std::path::PathBuf;

use agent_client_protocol::{AcpAgent, AcpAgentConfig};
use zz_protocol::{AGENT_CURSOR_COMMAND, AgentProvider};

use crate::agent::{
    dialect::{self, Dialect},
    environment::agent_path,
    host::RuntimeChannels,
    runtime::AgentSpawnConfig,
};

const SHORT_COMMAND: &str = "agent";
const NO_BROWSER: &str = "NO_OPEN_BROWSER";
const DIALECT: Dialect = Dialect {
    effort_option: "effort",
    signed_out: "Cursor is signed out. Sign in by running `cursor-agent login` in a terminal.",
    stored_title: |_| None,
    sign_in: Some("cursor_login"),
};

pub(crate) async fn run_cursor_runtime(
    config: AgentSpawnConfig,
    provider: AgentProvider,
    channels: RuntimeChannels,
) -> Result<(), String> {
    let inherited = std::env::var("PATH").ok();
    let agent = AcpAgent::new(
        AcpAgentConfig::new(program(agent_path().or(inherited.as_deref()))?)
            .arg("acp")
            .env(NO_BROWSER, "1"),
    );
    dialect::run(agent, DIALECT, config, provider, channels).await
}

fn program(path: Option<&str>) -> Result<PathBuf, String> {
    dialect::installed(path, AGENT_CURSOR_COMMAND)
        .into_iter()
        .next()
        .or_else(|| {
            dialect::installed(path, SHORT_COMMAND)
                .into_iter()
                .find(|candidate| {
                    std::fs::canonicalize(candidate).is_ok_and(|real| {
                        real.components()
                            .any(|part| part.as_os_str() == AGENT_CURSOR_COMMAND)
                    })
                })
        })
        .ok_or_else(|| {
            "Cursor's CLI is not installed: `cursor-agent` was not found on PATH. Install it with the script at https://cursor.com/install."
                .to_owned()
        })
}
