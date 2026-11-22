use std::path::PathBuf;

use agent_client_protocol::{AcpAgent, AcpAgentConfig};
use serde_json::Value;
use zz_protocol::{AGENT_GROK_COMMAND, AgentProvider, QuotaWindow, QuotaWindows};

use crate::agent::{
    dialect::{self, Dialect, Quota},
    environment::agent_path,
    host::RuntimeChannels,
    runtime::AgentSpawnConfig,
    session_files,
};

pub(crate) const DIALECT: Dialect = Dialect {
    effort_option: "reasoning_effort",
    signed_out: "Grok is signed out. Sign in by running `grok login` in a terminal.",
    stored_title,
    sign_in: None,
    model_name: None,
    unguarded: None,
    quota: Some(Quota {
        method: "_x.ai/billing",
        read: credit_usage,
    }),
    rules: Some("rules"),
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

fn credit_usage(billing: &Value) -> Option<QuotaWindows> {
    let config = &billing["config"];
    let window = QuotaWindow {
        used: (config["creditUsagePercent"].as_f64()? / 100.0) as f32,
        resets_at: config["currentPeriod"]["end"]
            .as_str()
            .and_then(|end| chrono::DateTime::parse_from_rfc3339(end).ok())
            .and_then(|end| u64::try_from(end.timestamp()).ok()),
    };
    (config["currentPeriod"]["type"] == "USAGE_PERIOD_TYPE_WEEKLY").then_some(QuotaWindows {
        five_hour: None,
        week: Some(window),
    })
}

fn stored_title(session: &str) -> Option<String> {
    let summary = session_files::grok_summary(&home()?, session)?;
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn weekly_credit_usage_is_the_week_window() {
        let billing = json!({
            "config": {
                "creditUsagePercent": 76.0,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-11-14T13:00:27.313+00:00",
                    "end": "2026-11-21T13:00:27.313+00:00",
                },
            },
            "subscription_tier": "SuperGrok Plus",
        });
        assert_eq!(
            credit_usage(&billing),
            Some(QuotaWindows {
                five_hour: None,
                week: Some(QuotaWindow {
                    used: 0.76,
                    resets_at: Some(1_795_266_027),
                }),
            })
        );
    }
}
